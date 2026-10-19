//! One declaration (RFC 002 rule 50, §3.1): the `florn.manifest` section and the config decoder
//! are generated from it, so that the manifest and the decoder cannot disagree.
//!
//! ```ignore
//! florn_guest::declare! {
//!     world: "florn:core/guest@0.1.0",
//!     kind: "router",
//!     description: "OSPF single-area router",
//!     ports: [("eth", "network/ethernet", 1, 8)],
//!     params: [
//!         (router_id, "router-id", text, true),
//!         (hello, "hello", ticks, false, default = "10s"),
//!         (cost, "cost", u64, false, default = 10),
//!         (price, "price", integer, false, default = "12.34USD", unit = "0.01USD"),
//!     ],
//!     telemetry: [(1, "neighbour-state", NeighbourState, [(neighbour, "neighbour", u64), (state, "state", u64)])],
//!     resources: [("memory", "4MiB"), ("pending-timers", 16)],
//! }
//! ```
//!
//! This generates `MANIFEST` (the TOML text), the `florn.manifest` custom section, `Config` with
//! one typed field per parameter and `Config::decode`, and one record type per channel with a
//! `report` method. Each parameter and field has a Rust identifier and the name the scenario
//! writes (`router_id` and `"router-id"`).

use alloc::string::String;
use alloc::vec::Vec;

/// A reader of the config record (RFC 002 rule 13): the values in declaration order. A short or
/// malformed record is a contract violation the Host never produces; it traps.
pub struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }
    fn take(&mut self, n: usize) -> &'a [u8] {
        match self.bytes.get(self.pos..self.pos + n) {
            Some(s) => {
                self.pos += n;
                s
            }
            None => crate::actor::trap(),
        }
    }
    pub fn bool(&mut self) -> bool {
        self.take(1)[0] != 0
    }
    pub fn u64(&mut self) -> u64 {
        u64::from_le_bytes(self.take(8).try_into().unwrap_or([0; 8]))
    }
    pub fn s64(&mut self) -> i64 {
        self.u64() as i64
    }
    pub fn f64(&mut self) -> f64 {
        f64::from_bits(self.u64())
    }
    pub fn bytes(&mut self) -> Vec<u8> {
        let n = u32::from_le_bytes(self.take(4).try_into().unwrap_or([0; 4])) as usize;
        self.take(n).to_vec()
    }
    pub fn text(&mut self) -> String {
        match String::from_utf8(self.bytes()) {
            Ok(s) => s,
            Err(_) => crate::actor::trap(),
        }
    }
    pub fn done(&self) -> bool {
        self.pos == self.bytes.len()
    }
}

/// The bytes of a `&str` as an array, for the link section.
pub const fn section<const N: usize>(s: &str) -> [u8; N] {
    let b = s.as_bytes();
    let mut out = [0u8; N];
    let mut i = 0;
    while i < N {
        out[i] = b[i];
        i += 1;
    }
    out
}

#[doc(hidden)]
#[macro_export]
macro_rules! __florn_ty {
    (bool) => {
        bool
    };
    (u64) => {
        u64
    };
    (s64) => {
        i64
    };
    (f64) => {
        f64
    };
    (text) => {
        $crate::__alloc::string::String
    };
    (bytes) => {
        $crate::__alloc::vec::Vec<u8>
    };
    (ticks) => {
        u64
    };
    (integer) => {
        i64
    };
    (real) => {
        f64
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __florn_read {
    ($r:ident, bool) => {
        $r.bool()
    };
    ($r:ident, u64) => {
        $r.u64()
    };
    ($r:ident, s64) => {
        $r.s64()
    };
    ($r:ident, f64) => {
        $r.f64()
    };
    ($r:ident, text) => {
        $r.text()
    };
    ($r:ident, bytes) => {
        $r.bytes()
    };
    ($r:ident, ticks) => {
        $r.u64()
    };
    ($r:ident, integer) => {
        $r.s64()
    };
    ($r:ident, real) => {
        $r.f64()
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __florn_write {
    ($e:ident, $v:expr, bool) => {
        $e.bool($v)
    };
    ($e:ident, $v:expr, u64) => {
        $e.u64($v)
    };
    ($e:ident, $v:expr, s64) => {
        $e.s64($v)
    };
    ($e:ident, $v:expr, f64) => {
        $e.f64($v)
    };
    ($e:ident, $v:expr, text) => {
        $e.text(&$v)
    };
    ($e:ident, $v:expr, bytes) => {
        $e.bytes(&$v)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __florn_default_line {
    () => {
        ""
    };
    ($d:literal) => {
        concat!("default = ", stringify!($d), "\n")
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __florn_unit_line {
    () => {
        ""
    };
    ($u:literal) => {
        concat!("unit = ", stringify!($u), "\n")
    };
}

/// The declaration of RFC 002 rule 12. See the module documentation for the form.
#[macro_export]
macro_rules! declare {
    (
        world: $world:literal,
        kind: $kind:literal,
        description: $description:literal,
        ports: [ $( ( $pk:literal, $pt:literal, $pmin:literal, $pmax:literal ) ),* $(,)? ],
        params: [ $( ( $pn:ident, $pname:literal, $pty:ident, $preq:literal $(, default = $pdef:literal)? $(, unit = $punit:literal)? ) ),* $(,)? ],
        telemetry: [ $( ( $ch:literal, $chname:literal, $chty:ident, [ $( ( $fname:ident, $fieldname:literal, $fty:ident ) ),* $(,)? ] ) ),* $(,)? ],
        resources: [ $( ( $rk:literal, $rv:literal ) ),* $(,)? ] $(,)?
    ) => {
        /// The manifest, as the `florn.manifest` section carries it.
        pub const MANIFEST: &str = concat!(
            "florn-module = 1\n",
            "world = ", stringify!($world), "\n",
            "kind = ", stringify!($kind), "\n",
            "description = ", stringify!($description), "\n",
            "sdk = \"florn-guest ", env!("CARGO_PKG_VERSION"), "\"\n",
            $(
                "\n[[ports]]\nkind = ", stringify!($pk), "\ntype = ", stringify!($pt),
                "\nmin = ", stringify!($pmin), "\nmax = ", stringify!($pmax), "\n",
            )*
            $(
                "\n[[params]]\nname = ", stringify!($pname), "\ntype = \"",
                stringify!($pty), "\"\nrequired = ", stringify!($preq), "\n",
                $crate::__florn_default_line!($($pdef)?),
                $crate::__florn_unit_line!($($punit)?),
            )*
            $(
                "\n[[telemetry]]\nchannel = ", stringify!($ch), "\nname = ", stringify!($chname),
                "\nfields = [", $( "{ name = ", stringify!($fieldname), ", type = \"", stringify!($fty), "\" }, ", )* "]\n",
            )*
            "\n[resources]\n",
            $( stringify!($rk), " = ", stringify!($rv), "\n", )*
        );

        /// The `florn.manifest` custom section (RFC 002 rule 11). The section exists in the
        /// WebAssembly module only; on another target the static is plain data (a Mach-O
        /// section needs a segment name, and nothing reads it there).
        ///
        /// SAFETY of the attribute: a custom section named for florn, carrying UTF-8 text; it
        /// names no section the linker or the runtime interprets.
        #[used]
        #[cfg_attr(target_arch = "wasm32", unsafe(link_section = "florn.manifest"))]
        pub static FLORN_MANIFEST: [u8; MANIFEST.len()] = $crate::manifest::section(MANIFEST);

        /// The declared parameters, as `init` receives them (RFC 002 rules 13 and 14).
        #[derive(Clone, Debug, PartialEq)]
        pub struct Config {
            $( pub $pn: $crate::__florn_ty!($pty), )*
        }

        impl Config {
            /// Decode the `config` argument of `init`: the values in declaration order, defaults
            /// filled by the Host.
            pub fn decode(bytes: &[u8]) -> Self {
                let mut r = $crate::Reader::new(bytes);
                let c = Self {
                    $( $pn: $crate::__florn_read!(r, $pty), )*
                };
                if !r.done() {
                    $crate::actor::trap();
                }
                c
            }
        }

        $(
            /// A record of the declared channel; `report` encodes and reports it.
            #[derive(Clone, Debug, PartialEq)]
            pub struct $chty {
                $( pub $fname: $crate::__florn_ty!($fty), )*
            }

            impl $chty {
                pub const CHANNEL: u32 = $ch;
                pub fn encode(&self) -> $crate::__alloc::vec::Vec<u8> {
                    let mut e = $crate::telemetry::Encoder::default();
                    $( $crate::__florn_write!(e, self.$fname, $fty); )*
                    e.finish()
                }
                pub fn report(&self) {
                    $crate::host::report(Self::CHANNEL, &self.encode());
                }
            }
        )*
    };
}
