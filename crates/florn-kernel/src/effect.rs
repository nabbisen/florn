//! Effects and the sink that records them during an activation (RFC 001 rules 22 to 24; RFC 002
//! §5; RFC 003 rule 17). A Host function only appends to the sink or refuses; nothing is applied
//! while the guest runs. The sink enforces the quotas that can be judged per call; the pending-timer
//! quota is judged at commit by the kernel.

/// One effect a guest asked for (RFC 002 §4, `host` interface).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Send { port: u32, payload: Vec<u8> },
    SetTimer { timer: u32, delay: u64 },
    CancelTimer { timer: u32 },
    Report { channel: u32, payload: Vec<u8> },
}

/// The quotas of RFC 003 §5 that the sink enforces per call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quotas {
    pub effects_per_activation: usize,
    pub payload_bytes: usize,
    pub telemetry_per_activation_bytes: usize,
    pub pending_timers: usize,
}

impl Default for Quotas {
    /// The provisional defaults of RFC 003 rule 19.
    fn default() -> Self {
        Self {
            effects_per_activation: 1_000,
            payload_bytes: 65_536,
            telemetry_per_activation_bytes: 16_384,
            pending_timers: 64,
        }
    }
}

/// Why a Host function refused a call. A refusal is the activation's fault (RFC 002 rule 40).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SinkRefusal {
    /// `fault.quota`: the named quota would be exceeded.
    Quota { quota: &'static str, limit: u64 },
    /// `fault.contract`: the call violates the contract (rule named).
    Contract { rule: &'static str, detail: String },
}

/// The record of effects of the current activation, with its quotas.
#[derive(Clone, Debug)]
pub struct Sink {
    quotas: Quotas,
    effects: Vec<Effect>,
    telemetry_bytes: usize,
    refusal: Option<SinkRefusal>,
    /// The ports this actor has (RFC 002 rule 23); a send through another is a contract violation.
    ports: Vec<u32>,
    /// The telemetry channels the module declares (RFC 002 rule 32); channel 0 is always allowed.
    channels: Vec<u32>,
}

impl Sink {
    pub fn new(quotas: Quotas, ports: Vec<u32>, channels: Vec<u32>) -> Self {
        Self {
            quotas,
            effects: Vec::new(),
            telemetry_bytes: 0,
            refusal: None,
            ports,
            channels,
        }
    }

    /// Start a new activation: nothing recorded, nothing refused.
    pub fn reset(&mut self) {
        self.effects.clear();
        self.telemetry_bytes = 0;
        self.refusal = None;
    }

    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }
    pub fn refusal(&self) -> Option<&SinkRefusal> {
        self.refusal.as_ref()
    }
    pub fn quotas(&self) -> &Quotas {
        &self.quotas
    }
    /// Take the recorded effects and the refusal, leaving the sink empty.
    pub fn take(&mut self) -> (Vec<Effect>, Option<SinkRefusal>) {
        self.telemetry_bytes = 0;
        (std::mem::take(&mut self.effects), self.refusal.take())
    }

    fn room(&mut self) -> Result<(), SinkRefusal> {
        if self.effects.len() >= self.quotas.effects_per_activation {
            let r = SinkRefusal::Quota {
                quota: "effects-per-activation",
                limit: self.quotas.effects_per_activation as u64,
            };
            self.refusal = Some(r.clone());
            return Err(r);
        }
        Ok(())
    }
    fn refuse(&mut self, r: SinkRefusal) -> Result<(), SinkRefusal> {
        self.refusal = Some(r.clone());
        Err(r)
    }

    /// `send` (RFC 002 rule 23).
    pub fn send(&mut self, port: u32, payload: Vec<u8>) -> Result<(), SinkRefusal> {
        if !self.ports.contains(&port) {
            return self.refuse(SinkRefusal::Contract {
                rule: "RFC 002 rule 23",
                detail: format!("port {port} is not a port of this actor"),
            });
        }
        if payload.len() > self.quotas.payload_bytes {
            return self.refuse(SinkRefusal::Quota {
                quota: "payload",
                limit: self.quotas.payload_bytes as u64,
            });
        }
        self.room()?;
        self.effects.push(Effect::Send { port, payload });
        Ok(())
    }

    /// `set-timer` (RFC 002 rule 26). The delay's range is checked at commit, where the instant is known.
    pub fn set_timer(&mut self, timer: u32, delay: u64) -> Result<(), SinkRefusal> {
        self.room()?;
        self.effects.push(Effect::SetTimer { timer, delay });
        Ok(())
    }

    /// `cancel-timer` (RFC 002 rule 28).
    pub fn cancel_timer(&mut self, timer: u32) -> Result<(), SinkRefusal> {
        self.room()?;
        self.effects.push(Effect::CancelTimer { timer });
        Ok(())
    }

    /// `report` (RFC 002 rules 32, 33).
    pub fn report(&mut self, channel: u32, payload: Vec<u8>) -> Result<(), SinkRefusal> {
        if channel != 0 && !self.channels.contains(&channel) {
            return self.refuse(SinkRefusal::Contract {
                rule: "RFC 002 rule 32",
                detail: format!("channel {channel} is not declared"),
            });
        }
        if self.telemetry_bytes + payload.len() > self.quotas.telemetry_per_activation_bytes {
            return self.refuse(SinkRefusal::Quota {
                quota: "telemetry-per-activation",
                limit: self.quotas.telemetry_per_activation_bytes as u64,
            });
        }
        self.room()?;
        self.telemetry_bytes += payload.len();
        self.effects.push(Effect::Report { channel, payload });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotas_and_contract() {
        let mut s = Sink::new(
            Quotas {
                effects_per_activation: 2,
                payload_bytes: 4,
                telemetry_per_activation_bytes: 5,
                pending_timers: 1,
            },
            vec![0],
            vec![1],
        );
        assert!(s.send(0, vec![1, 2, 3, 4]).is_ok());
        assert_eq!(
            s.send(1, vec![]).unwrap_err(),
            SinkRefusal::Contract {
                rule: "RFC 002 rule 23",
                detail: "port 1 is not a port of this actor".into()
            }
        );
        s.reset();
        assert_eq!(
            s.send(0, vec![0; 5]).unwrap_err(),
            SinkRefusal::Quota {
                quota: "payload",
                limit: 4
            }
        );
        s.reset();
        assert!(s.report(1, vec![0; 3]).is_ok());
        assert_eq!(
            s.report(0, vec![0; 3]).unwrap_err(),
            SinkRefusal::Quota {
                quota: "telemetry-per-activation",
                limit: 5
            }
        );
        s.reset();
        assert!(s.set_timer(0, 1).is_ok());
        assert!(s.cancel_timer(0).is_ok());
        assert_eq!(
            s.set_timer(0, 1).unwrap_err(),
            SinkRefusal::Quota {
                quota: "effects-per-activation",
                limit: 2
            }
        );
        assert_eq!(
            s.report(2, vec![]).unwrap_err(),
            SinkRefusal::Contract {
                rule: "RFC 002 rule 32",
                detail: "channel 2 is not declared".into()
            }
        );
    }
}
