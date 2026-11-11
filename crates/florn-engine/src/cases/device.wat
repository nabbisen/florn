;; The guest of the ABI cases (PR-37; RFC 012 findings F-1 and F-16): the calls the circuit domain
;; needs, with the bodies of a test. It stands for the device and the controller of that domain:
;;
;;   begin-step(start: i64, h: f64)       stores both, and calls set-output(7, h * 2)
;;   on-sample(now: i64, index: i64)      stores both
;;   reject-step()                        counts
;;   evaluate(inputs: list<f64>) -> list<f64>
;;                                         each output is input * 2 + 0.5; an empty list gives one
;;                                         NaN that is not the canonical one (0x7ff8dead00000001)
;;   cabi_post_evaluate(ptr)              frees what evaluate and the lowering of its argument
;;                                         allocated
;;
;; Its allocator is a bump allocator. A call of evaluate and its post-return bring `top` back to
;; where it was, so a Host that does not call the post-return makes `top` and the memory grow, and
;; a Host that does leaves both as they were.
;;
;; The scratch cells (bytes 0 to 63): 0 start, 8 h, 16 now, 24 index, 32 the reject count, 40 the
;; argument of the last post-return call, 44 how many there were, 48 the pointer the last
;; evaluate returned.
(module
  (import "florn:circuit/host@0.1.0" "set-output" (func $set_output (param i32 f64)))
  ;; The Host's budget counter (RFC 003 rule 12). The guest is not instrumented; it charges itself a
  ;; fixed amount at each of its functions, so that the budget a case reads after a sequence of
  ;; calls is a number the case can state: cabi_realloc 1, begin-step 10, evaluate 100, the
  ;; post-return of evaluate 1000.
  (import "env" "gas_counter" (global $gas (mut i64)))
  (memory (export "memory") 1)
  (global $top (export "top") (mut i32) (i32.const 1024))
  (global $mark (mut i32) (i32.const 0))

  (func $realloc (export "cabi_realloc") (param $old_ptr i32) (param $old_size i32) (param $align i32) (param $size i32) (result i32)
    (local $p i32)
    (global.set $gas (i64.sub (global.get $gas) (i64.const 1)))
    (local.set $p (i32.and (i32.add (global.get $top) (i32.sub (local.get $align) (i32.const 1)))
                           (i32.sub (i32.const 0) (local.get $align))))
    (global.set $top (i32.add (local.get $p) (local.get $size)))
    (block $fits
      (loop $grow
        (br_if $fits (i32.le_u (global.get $top) (i32.mul (memory.size) (i32.const 65536))))
        (drop (memory.grow (i32.const 1)))
        (br $grow)))
    (local.get $p))

  (func (export "florn:circuit/device@0.1.0#begin-step") (param $start i64) (param $h f64)
    (global.set $gas (i64.sub (global.get $gas) (i64.const 10)))
    (i64.store (i32.const 0) (local.get $start))
    (f64.store (i32.const 8) (local.get $h))
    (call $set_output (i32.const 7) (f64.mul (local.get $h) (f64.const 2))))

  (func (export "florn:circuit/controller@0.1.0#on-sample") (param $now i64) (param $index i64)
    (i64.store (i32.const 16) (local.get $now))
    (i64.store (i32.const 24) (local.get $index)))

  (func (export "florn:circuit/device@0.1.0#reject-step")
    (i32.store (i32.const 32) (i32.add (i32.load (i32.const 32)) (i32.const 1))))

  (func (export "florn:circuit/device@0.1.0#evaluate") (param $ptr i32) (param $len i32) (result i32)
    (local $area i32)
    (local $out i32)
    (local $i i32)
    (global.set $gas (i64.sub (global.get $gas) (i64.const 100)))
    ;; the lowering of the argument allocated at $ptr; the post-return frees from there
    (global.set $mark (local.get $ptr))
    (local.set $area (call $realloc (i32.const 0) (i32.const 0) (i32.const 4) (i32.const 8)))
    (if (i32.eqz (local.get $len))
      (then
        (local.set $out (call $realloc (i32.const 0) (i32.const 0) (i32.const 8) (i32.const 8)))
        (i64.store (local.get $out) (i64.const 0x7ff8dead00000001))
        (i32.store (local.get $area) (local.get $out))
        (i32.store offset=4 (local.get $area) (i32.const 1)))
      (else
        (local.set $out (call $realloc (i32.const 0) (i32.const 0) (i32.const 8)
                              (i32.mul (local.get $len) (i32.const 8))))
        (block $done
          (loop $each
            (br_if $done (i32.ge_u (local.get $i) (local.get $len)))
            (f64.store
              (i32.add (local.get $out) (i32.mul (local.get $i) (i32.const 8)))
              (f64.add
                (f64.mul
                  (f64.load (i32.add (local.get $ptr) (i32.mul (local.get $i) (i32.const 8))))
                  (f64.const 2))
                (f64.const 0.5)))
            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            (br $each)))
        (i32.store (local.get $area) (local.get $out))
        (i32.store offset=4 (local.get $area) (local.get $len))))
    (i32.store (i32.const 48) (local.get $area))
    (local.get $area))

  (func (export "cabi_post_florn:circuit/device@0.1.0#evaluate") (param $area i32)
    (global.set $gas (i64.sub (global.get $gas) (i64.const 1000)))
    (i32.store (i32.const 40) (local.get $area))
    (i32.store (i32.const 44) (i32.add (i32.load (i32.const 44)) (i32.const 1)))
    (global.set $top (global.get $mark))))
