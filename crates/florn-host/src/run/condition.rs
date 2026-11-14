//! Why a Host fault ended the run (RFC 001 rule 65): the `reason` of `host.not-reproducible`, a
//! value of the closed enumeration `machine-condition` (RFC 007 section 4.5). A Host fault carries
//! a value of this type and nothing else, so that no code can give a value outside the set.

use florn_engine::TrapKind;

/// A value of `machine-condition` that the Host gives for a fault of its own. (`memory-refused`
/// belongs to the set too; no activation gives it, an instance that cannot be made resident
/// being `host.instance-failed`.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MachineCondition {
    /// The engine's own stack was exhausted.
    EngineStack,
    /// The engine's watchdog stopped a call (ACT-006).
    Watchdog,
    /// The engine could not do what the Host asked of it: a trap of no other kind, or a global
    /// or a memory it would not give.
    EngineFailure,
    /// A defect of the Host or of a domain, such as a compound activation left open.
    HostDefect,
}

impl MachineCondition {
    /// The value's name in the enumeration (and in `enum.machine-condition.<name>`).
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::EngineStack => "engine-stack",
            Self::Watchdog => "watchdog",
            Self::EngineFailure => "engine-failure",
            Self::HostDefect => "host-defect",
        }
    }

    /// The condition of a trap that is the Host's fault and not the guest's; `None` for a trap
    /// of the guest or a refusal of the Host's own import.
    pub(super) fn of_trap(kind: TrapKind) -> Option<Self> {
        match kind {
            TrapKind::EngineStack => Some(Self::EngineStack),
            TrapKind::Watchdog => Some(Self::Watchdog),
            TrapKind::Other => Some(Self::EngineFailure),
            TrapKind::Guest | TrapKind::HostRefused => None,
        }
    }
}

#[cfg(test)]
mod tests;
