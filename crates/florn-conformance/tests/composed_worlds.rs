//! PR-39, several and composed worlds (RFC 002 rule 48; RFC 008 rule 6; RFC 012 finding F-9),
//! through the kit's two test domains: a module whose world is the test domain's and a second
//! domain's together is admitted when the Host holds both domains and refused when it holds one;
//! a scenario that uses it declares both.
use florn_admission::Admission;
use florn_conformance::host;
use florn_conformance::test_domain::second::{self, COMPOSED, SecondDomain};
use florn_conformance::test_domain::{self as td, TestDomain};
use florn_host::{Domains, HostError, Options, Run, Seed};
use florn_platform::FixedPlatform;

fn composed_module() -> Vec<u8> {
    td::guest_bytes_in(&second::composed_wat(), "both", COMPOSED)
}

fn pinger_module() -> Vec<u8> {
    td::guest_bytes_in(&second::pinger_wat(), "pinger", second::WORLD)
}

/// The codes with which `bytes` is admitted or refused by an admission step that has the worlds
/// of `domains`.
fn admit(domains: &Domains, bytes: &[u8]) -> Vec<String> {
    let mut adm = Admission::default();
    for w in domains.worlds() {
        adm.add_world(w);
    }
    match adm.admit("m", bytes, &host::kit_limits()) {
        Ok(_) => Vec::new(),
        Err(e) => e.codes().iter().map(|c| c.to_string()).collect(),
    }
}

fn both() -> Domains {
    second::domains()
}

/// The test domain alone, with the composed world registered, as a shell without the second
/// domain would build it.
fn test_only() -> Domains {
    Domains::core().with(TestDomain::new).with_composed(
        COMPOSED,
        &[("test", td::WORLD), (second::NAME, second::WORLD)],
    )
}

fn second_only() -> Domains {
    Domains::core().with(SecondDomain::new).with_composed(
        COMPOSED,
        &[("test", td::WORLD), (second::NAME, second::WORLD)],
    )
}

// ---------------------------------------------------------------------------------------------
// Admission: the world is registered when the Host holds every domain it spans.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_module_of_a_composed_world_is_admitted_with_both_domains_present() {
    assert_eq!(admit(&both(), &composed_module()), Vec::<String>::new());
}

#[test]
fn it_is_refused_with_one_of_them() {
    for held in [test_only(), second_only()] {
        // The world is not one this Host offers; the imports of a module of no world known are
        // refused with it.
        let codes = admit(&held, &composed_module());
        assert!(
            codes.contains(&"module.contract-unsupported".to_string()),
            "{held:?}: {codes:?}"
        );
    }
}

#[test]
fn each_domains_own_world_is_still_admitted_alone() {
    assert!(admit(&test_only(), &td::guest_bytes()).is_empty());
    assert!(admit(&second_only(), &pinger_module()).is_empty());
    // And the second domain's module on a Host that has only the test domain is refused, as
    // before: its world is not one the Host offers.
    assert!(
        admit(&test_only(), &pinger_module()).contains(&"module.contract-unsupported".to_string())
    );
}

#[test]
fn a_module_of_the_composed_world_must_have_the_exports_of_both_parts() {
    // Without the second domain's `on-ping` or without the test domain's `on-wire`, the module
    // does not do what its world says.
    let wat = second::composed_wat();
    let without_ping = wat.replace(
        "(func (export \"test:kit/pings@0.1.0#on-ping\") (param i64 i32 i32))",
        "",
    );
    assert_ne!(without_ping, wat);
    assert_eq!(
        admit(
            &both(),
            &td::guest_bytes_in(&without_ping, "both", COMPOSED)
        ),
        vec!["module.export-missing"]
    );
    let without_wire = wat.replace(
        "test:kit/events@0.1.0#on-wire",
        "test:kit/events@0.1.0#other",
    );
    assert_ne!(without_wire, wat);
    assert_eq!(
        admit(
            &both(),
            &td::guest_bytes_in(&without_wire, "both", COMPOSED)
        ),
        vec!["module.export-missing"]
    );
}

// ---------------------------------------------------------------------------------------------
// A scenario declares every domain the module's world uses (RFC 002 rule 48; RFC 004 rule 5).
// ---------------------------------------------------------------------------------------------

/// A scenario text declaring `domains`, with one module `prober` of `bytes` and `actors`.
fn scenario(domains: &[&str], bytes: &[u8], actors: &str) -> String {
    let hash = florn_host::run::sha256_hex(bytes);
    let declared: Vec<String> = domains.iter().map(|d| format!("\"{d}\"")).collect();
    format!(
        "florn = 1\nname = \"composed\"\ndomains = [{}]\n\n[time]\ntick = \"1ns\"\nend = \"100ns\"\n\n[order]\ntie = \"by-identity\"\n\n[seed]\nroot = 3\n\n[limits.default]\nmemory = \"2MiB\"\n\n[modules.prober]\nsha256 = \"{hash}\"\ncontract = \"{}\"\n{actors}",
        declared.join(", "),
        if bytes == composed_module() {
            COMPOSED
        } else if bytes == pinger_module() {
            second::WORLD
        } else {
            td::WORLD
        },
    )
}

/// Two probers on one wire, as the kit's other scenarios have them.
fn wired() -> String {
    format!(
        "{}{}{}",
        td::prober(1, 1),
        td::prober(2, 1),
        td::wire(100, &[(1, 0), (2, 0)], "")
    )
}

const LONE: &str =
    "\n[[actors]]\nid = 1\nname = \"a1\"\nkind = \"module\"\nmodule = \"prober\"\nports = []\n";

fn start(domains: &Domains, text: &str, bytes: &[u8]) -> Result<host::Host, HostError> {
    let manifest = td::manifest(text);
    let seed = manifest.seed;
    let modules = [("prober".to_string(), bytes.to_vec())]
        .into_iter()
        .collect();
    // As `Controller::new` does, before any scenario: the worlds of every domain the Host holds.
    let mut admission = Admission::default();
    for w in domains.worlds() {
        admission.add_world(w);
    }
    Run::new(
        host::engine(),
        admission,
        &mut FixedPlatform::default(),
        domains,
        manifest,
        &modules,
        Seed::Given(seed),
        Options {
            keep_records: true,
            ..Options::default()
        },
    )
}

fn refused(r: Result<host::Host, HostError>) -> Vec<(String, String)> {
    match r {
        Err(HostError::Refused(e)) => {
            e.0.iter()
                .map(|x| {
                    let domain = x
                        .params()
                        .into_iter()
                        .find(|(k, _)| *k == "domain")
                        .map(|(_, v)| format!("{v:?}"))
                        .unwrap_or_default();
                    (x.code().to_string(), domain)
                })
                .collect()
        }
        Err(other) => vec![(other.to_string(), String::new())],
        Ok(_) => Vec::new(),
    }
}

#[test]
fn a_scenario_that_declares_both_domains_runs_the_composed_module() {
    let bytes = composed_module();
    let text = scenario(&["test", "second"], &bytes, &wired());
    let mut run = start(&both(), &text, &bytes).expect("both declared: it starts");
    run.run_to_end();
    assert!(host::faults(&run).is_empty());
}

#[test]
fn a_scenario_that_declares_one_of_them_is_refused_naming_the_other() {
    let bytes = composed_module();
    for (declared, missing) in [(["test"], "second"), (["second"], "test")] {
        let text = scenario(&declared, &bytes, LONE);
        let r = refused(start(&both(), &text, &bytes));
        assert_eq!(r.len(), 1, "{declared:?}: {r:?}");
        assert_eq!(r[0].0, "domain.undeclared", "{declared:?}");
        assert!(r[0].1.contains(missing), "{declared:?}: {r:?}");
    }
}

#[test]
fn a_domains_own_world_needs_its_domain_declared_too() {
    // The same rule for a world of one domain: the second domain's module in a scenario that
    // declares the test domain only is refused, and starts when the second is declared too.
    let bytes = pinger_module();
    let r = refused(start(&both(), &scenario(&["test"], &bytes, LONE), &bytes));
    assert_eq!(r.len(), 1, "{r:?}");
    assert_eq!(r[0].0, "domain.undeclared");
    assert!(r[0].1.contains("second"), "{r:?}");
    let ok = start(
        &both(),
        &scenario(&["test", "second"], &bytes, LONE),
        &bytes,
    );
    assert!(ok.is_ok(), "{:?}", refused(ok));
}

#[test]
fn a_host_that_lacks_a_domain_the_scenario_declares_refuses_it_as_unknown() {
    let bytes = composed_module();
    let text = scenario(&["test", "second"], &bytes, LONE);
    let r = refused(start(&test_only(), &text, &bytes));
    assert_eq!(r.len(), 1, "{r:?}");
    assert_eq!(r[0].0, "domain.unknown");
}

#[test]
fn the_digest_of_a_run_of_the_composed_module_is_that_of_the_same_run_again() {
    let bytes = composed_module();
    let text = scenario(&["test", "second"], &bytes, &wired());
    let digest = || {
        let mut run = start(&both(), &text, &bytes).unwrap();
        run.run_to_end();
        run.chain_hex()
    };
    assert_eq!(digest(), digest());
}
