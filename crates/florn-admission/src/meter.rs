//! Cost-model version 2 (RFC 010): florn's own metering pass. It charges the units of version 1
//! (one per instruction, nothing for `end` and `else`) at the charge points of version 1 (the
//! metered blocks of the library pass, whose partition this module reproduces), but into a local
//! of the function (rule 2), four instructions that touch no global; the budget global is
//! touched only at the checks of rule 3: before every `call` and `call_indirect` (which covers
//! the callee's entry), before a branch to a `loop` label (a back-edge), before `return` and at
//! the function's end, where the local is subtracted from the global, reset, and the global
//! compared with zero. An activation that does not fault is therefore metered as under version
//! 1, instruction for instruction; one that traps between two checks is metered without the
//! local's remainder, which version 1 had charged at the block's start (rule 1). Between two
//! checks a function executes each of its instructions at most once, so the budget is overrun
//! by at most the function's length, which admission bounds by the class's budget.
//!
//! The budget global is version 1's import (`env.gas_counter`, index 0 of the global index
//! space; every other global moves up by one), so that neither Host changes its plumbing; a
//! trap leaves the global below zero, which the Host classifies as the `budget` fault (RFC 003
//! rule 13). An effect reaches the Host only through a call, and a check precedes every call,
//! so nothing is committed past the budget. The library pass and an inline charge on the global
//! cost the same in the browsers' engines, where an imported mutable global is reached through
//! an indirection; the local is what makes the difference there.

use std::collections::BTreeMap;

use wasm_encoder::reencode::{self, Reencode};
use wasm_encoder::{
    BlockType, CodeSection, EntityType, Function, GlobalType, ImportSection, Instruction, Module,
    SectionId, ValType,
};
use wasmparser::{Operator, Parser, Payload, TypeRef};

use crate::{BUDGET_IMPORT_MODULE, BUDGET_IMPORT_NAME};

/// The cost model's table, recorded with the version in the run record (RFC 003 rule 14).
pub const COST_MODEL_V2_TABLE: &str = "\
cost-model = 2
pass = \"florn-admission meter\"
instruction = 1          # units per instruction, every instruction alike; end and else 0
memory-grow = 1          # units per memory.grow call; nothing per page
charge = 0               # units for the charge itself
blocks = \"the metered blocks of cost-model 1, charged into a local of the function\"
checks = \"before call and call_indirect, before a branch to a loop, before return, at the end\"
stack-unit = \"max operand stack and locals of a function, summed along the call chain\"
budget-global = \"env.gas_counter\"
height-global = \"florn.stack\"
";

/// What the pass produced: the module and the longest check-free path of any function, as
/// instructions (the bound of RFC 010 rule 3: between two checks a function executes each of
/// its instructions at most once).
pub struct Metered {
    pub bytes: Vec<u8>,
    pub longest_path: u64,
    /// The function with the longest path, by its index in the module.
    pub longest_function: u32,
}

/// The pass over a module without an imported global (the budget global becomes import 0).
pub fn meter(bytes: &[u8]) -> Result<Metered, String> {
    let facts = Facts::of(bytes)?;
    let mut pass = Pass {
        facts,
        seen_import: false,
        body: 0,
        longest: 0,
        longest_function: 0,
    };
    let mut module = Module::new();
    pass.parse_core_module(&mut module, Parser::new(0), bytes)
        .map_err(|e| e.to_string())?;
    Ok(Metered {
        bytes: module.finish(),
        longest_path: pass.longest,
        longest_function: pass.longest_function,
    })
}

/// What the pass knows before it reads a body: each type's parameter count, the imported
/// functions, each defined function's type (the accumulator's local index follows), and that
/// the module imports no global.
struct Facts {
    params: Vec<u32>,
    imported_functions: u32,
    function_types: Vec<u32>,
}

impl Facts {
    fn of(bytes: &[u8]) -> Result<Self, String> {
        let mut f = Facts {
            params: Vec::new(),
            imported_functions: 0,
            function_types: Vec::new(),
        };
        for payload in Parser::new(0).parse_all(bytes) {
            match payload.map_err(|e| e.to_string())? {
                Payload::TypeSection(r) => {
                    for t in r.into_iter_err_on_gc_types() {
                        f.params
                            .push(t.map_err(|e| e.to_string())?.params().len() as u32);
                    }
                }
                Payload::FunctionSection(r) => {
                    for t in r {
                        f.function_types.push(t.map_err(|e| e.to_string())?);
                    }
                }
                Payload::ImportSection(r) => {
                    for i in r {
                        if let wasmparser::Imports::Single(_, imp) = i.map_err(|e| e.to_string())? {
                            match imp.ty {
                                TypeRef::Func(_) => f.imported_functions += 1,
                                TypeRef::Global(_) => {
                                    return Err("the source imports a global".into());
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(f)
    }
}

struct Pass {
    facts: Facts,
    seen_import: bool,
    /// Bodies rewritten so far: the next body's index among the defined functions.
    body: usize,
    longest: u64,
    longest_function: u32,
}

/// The budget global as an import entity.
fn budget_global() -> EntityType {
    EntityType::Global(GlobalType {
        val_type: ValType::I64,
        mutable: true,
        shared: false,
    })
}

/// The units of one instruction (RFC 003 rule 14, as the library counts them).
fn units(op: &Operator<'_>) -> u64 {
    match op {
        Operator::End | Operator::Else => 0,
        _ => 1,
    }
}

/// The library's partition of a body into metered blocks (its `determine_metered_blocks`),
/// reproduced so that version 2 charges where version 1 charges: a metered block is executed
/// whole or not at all, blocks opened with `block` merge into the block before them, and a
/// control block that may be branched out of ends the enclosing metered block too.
struct Control {
    lowest_forward_target: usize,
    start: usize,
    cost: u64,
    is_loop: bool,
}

struct Blocks {
    stack: Vec<Control>,
    done: Vec<(usize, u64)>,
}

impl Blocks {
    fn begin(&mut self, cursor: usize, is_loop: bool) {
        let index = self.stack.len();
        self.stack.push(Control {
            lowest_forward_target: index,
            start: cursor,
            cost: 0,
            is_loop,
        });
    }
    fn increment(&mut self, units: u64) -> Result<(), String> {
        let top = self.stack.last_mut().ok_or("no control block")?;
        top.cost = top.cost.checked_add(units).ok_or("cost overflow")?;
        Ok(())
    }
    fn finalize_metered(&mut self, cursor: usize) -> Result<(), String> {
        let top = self.stack.last_mut().ok_or("no control block")?;
        let closing = (top.start, top.cost);
        top.start = cursor + 1;
        top.cost = 0;
        let last = self.stack.len() - 1;
        if last > 0 {
            let prev = &mut self.stack[last - 1];
            if closing.0 == prev.start {
                prev.cost += closing.1;
                return Ok(());
            }
        }
        if closing.1 > 0 {
            self.done.push(closing);
        }
        Ok(())
    }
    fn finalize_control(&mut self, cursor: usize) -> Result<(), String> {
        self.finalize_metered(cursor)?;
        let closing = self.stack.pop().ok_or("no control block")?;
        let index = self.stack.len();
        let Some(top) = self.stack.last_mut() else {
            return Ok(());
        };
        top.lowest_forward_target = top.lowest_forward_target.min(closing.lowest_forward_target);
        if closing.lowest_forward_target < index {
            self.finalize_metered(cursor)?;
        }
        Ok(())
    }
    fn branch(&mut self, cursor: usize, targets: &[usize]) -> Result<(), String> {
        self.finalize_metered(cursor)?;
        for &t in targets {
            if self.stack.get(t).ok_or("branch target")?.is_loop {
                continue;
            }
            let top = self.stack.last_mut().ok_or("no control block")?;
            top.lowest_forward_target = top.lowest_forward_target.min(t);
        }
        Ok(())
    }
}

/// The charges of a body: the units to subtract before the instruction at each start.
fn metered_blocks(ops: &[Operator<'_>]) -> Result<BTreeMap<usize, u64>, String> {
    let mut b = Blocks {
        stack: Vec::new(),
        done: Vec::new(),
    };
    b.begin(0, false);
    for (cursor, op) in ops.iter().enumerate() {
        let cost = units(op);
        match op {
            Operator::Block { .. } => {
                b.increment(cost)?;
                let start = b.stack.last().ok_or("no control block")?.start;
                b.begin(start, false);
            }
            Operator::If { .. } => {
                b.increment(cost)?;
                b.begin(cursor + 1, false);
            }
            Operator::Loop { .. } => {
                b.increment(cost)?;
                b.begin(cursor + 1, true);
            }
            Operator::End => b.finalize_control(cursor)?,
            Operator::Else => b.finalize_metered(cursor)?,
            Operator::Br { relative_depth } | Operator::BrIf { relative_depth } => {
                b.increment(cost)?;
                let active = b.stack.len().checked_sub(1).ok_or("no control block")?;
                let target = active
                    .checked_sub(*relative_depth as usize)
                    .ok_or("branch depth")?;
                b.branch(cursor, &[target])?;
            }
            Operator::BrTable { targets } => {
                b.increment(cost)?;
                let active = b.stack.len().checked_sub(1).ok_or("no control block")?;
                // As the library does: the listed targets, not the default.
                let mut indices = Vec::new();
                for t in targets.targets() {
                    let t = t.map_err(|e| e.to_string())?;
                    indices.push(active.checked_sub(t as usize).ok_or("branch depth")?);
                }
                b.branch(cursor, &indices)?;
            }
            Operator::Return => {
                b.increment(cost)?;
                b.branch(cursor, &[0])?;
            }
            _ => b.increment(cost)?,
        }
    }
    let mut charges = BTreeMap::new();
    for (start, cost) in b.done {
        *charges.entry(start).or_insert(0) += cost;
    }
    Ok(charges)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Label {
    Block,
    Loop,
}

impl Reencode for Pass {
    type Error = String;

    /// The budget global is import 0 of the global index space: every other global moves up.
    fn global_index(&mut self, global: u32) -> Result<u32, reencode::Error<Self::Error>> {
        Ok(global + 1)
    }

    fn parse_import_section(
        &mut self,
        imports: &mut ImportSection,
        section: wasmparser::ImportSectionReader<'_>,
    ) -> Result<(), reencode::Error<Self::Error>> {
        imports.import(BUDGET_IMPORT_MODULE, BUDGET_IMPORT_NAME, budget_global());
        self.seen_import = true;
        reencode::utils::parse_import_section(self, imports, section)
    }

    /// A module without imports gets an import section for the budget global, before the
    /// first section that must follow it.
    fn intersperse_section_hook(
        &mut self,
        module: &mut Module,
        _after: Option<SectionId>,
        before: Option<SectionId>,
    ) -> Result<(), reencode::Error<Self::Error>> {
        if !self.seen_import
            && let Some(b) = before
            && b != SectionId::Custom
            && (b as u8) > (SectionId::Import as u8)
        {
            let mut imports = ImportSection::new();
            imports.import(BUDGET_IMPORT_MODULE, BUDGET_IMPORT_NAME, budget_global());
            module.section(&imports);
            self.seen_import = true;
        }
        Ok(())
    }

    fn parse_function_body(
        &mut self,
        code: &mut CodeSection,
        func: wasmparser::FunctionBody<'_>,
    ) -> Result<(), reencode::Error<Self::Error>> {
        let index = self.body;
        self.body += 1;
        // The locals, then one more: the accumulator of rule 2.
        let type_index =
            *self.facts.function_types.get(index).ok_or_else(|| {
                reencode::Error::UserError("a body without a function".to_string())
            })?;
        let params =
            *self.facts.params.get(type_index as usize).ok_or_else(|| {
                reencode::Error::UserError("a function without a type".to_string())
            })?;
        let mut locals = Vec::new();
        let mut local_count = 0u32;
        for pair in func.get_locals_reader()? {
            let (count, ty) = pair?;
            local_count = local_count.saturating_add(count);
            locals.push((count, self.val_type(ty)?));
        }
        locals.push((1, ValType::I64));
        let acc = params.saturating_add(local_count);
        let mut f = Function::new(locals);

        // Every operator of the body: the partition needs them all before the first is emitted.
        let mut ops = Vec::new();
        let mut reader = func.get_operators_reader()?;
        while !reader.eof() {
            ops.push(reader.read()?);
        }
        let total = ops.len() as u64;
        if total > self.longest {
            self.longest = total;
            self.longest_function = self.facts.imported_functions + index as u32;
        }
        let charges = metered_blocks(&ops).map_err(reencode::Error::UserError)?;

        // A charge adds to the local; a check moves the local into the global, resets it, and
        // traps when the global is below zero.
        let charge = |f: &mut Function, cost: u64| {
            f.instruction(&Instruction::LocalGet(acc));
            f.instruction(&Instruction::I64Const(cost as i64));
            f.instruction(&Instruction::I64Add);
            f.instruction(&Instruction::LocalSet(acc));
        };
        let check = |f: &mut Function| {
            f.instruction(&Instruction::GlobalGet(0));
            f.instruction(&Instruction::LocalGet(acc));
            f.instruction(&Instruction::I64Sub);
            f.instruction(&Instruction::LocalTee(acc));
            f.instruction(&Instruction::GlobalSet(0));
            f.instruction(&Instruction::LocalGet(acc));
            f.instruction(&Instruction::I64Const(0));
            f.instruction(&Instruction::I64LtS);
            f.instruction(&Instruction::If(BlockType::Empty));
            f.instruction(&Instruction::Unreachable);
            f.instruction(&Instruction::End);
            f.instruction(&Instruction::I64Const(0));
            f.instruction(&Instruction::LocalSet(acc));
        };
        let to_loop = |depth: u32, labels: &[Label]| {
            labels
                .len()
                .checked_sub(1 + depth as usize)
                .is_some_and(|at| labels[at] == Label::Loop)
        };

        let mut labels: Vec<Label> = Vec::new();
        for (cursor, op) in ops.into_iter().enumerate() {
            if let Some(cost) = charges.get(&cursor) {
                charge(&mut f, *cost);
            }
            let needs_check = match &op {
                Operator::Call { .. } | Operator::CallIndirect { .. } | Operator::Return => true,
                Operator::Br { relative_depth } | Operator::BrIf { relative_depth } => {
                    to_loop(*relative_depth, &labels)
                }
                Operator::BrTable { targets } => {
                    let mut any = to_loop(targets.default(), &labels);
                    for t in targets.targets() {
                        any |= to_loop(t?, &labels);
                    }
                    any
                }
                Operator::End => labels.is_empty(),
                _ => false,
            };
            if needs_check {
                check(&mut f);
            }
            match &op {
                Operator::Block { .. } | Operator::If { .. } => labels.push(Label::Block),
                Operator::Loop { .. } => labels.push(Label::Loop),
                Operator::End => {
                    labels.pop();
                }
                _ => {}
            }
            f.instruction(&self.instruction(op)?);
        }
        code.function(&f);
        Ok(())
    }
}
