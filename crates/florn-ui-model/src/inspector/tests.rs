use super::*;

#[test]
fn a_route_report_decodes_by_its_layout() {
    let layout = vec![
        ("prefix".to_string(), "text".to_string()),
        ("next-hop".to_string(), "u64".to_string()),
        ("metric".to_string(), "u64".to_string()),
        ("action".to_string(), "text".to_string()),
    ];
    let mut p = Vec::new();
    p.extend_from_slice(&2u32.to_le_bytes());
    p.extend_from_slice(b"r3");
    p.extend_from_slice(&2u64.to_le_bytes());
    p.extend_from_slice(&1u64.to_le_bytes());
    p.extend_from_slice(&5u32.to_le_bytes());
    p.extend_from_slice(b"added");
    assert_eq!(
        decode_report(&layout, &p),
        vec![
            ("prefix".into(), "r3".into()),
            ("next-hop".into(), "2".into()),
            ("metric".into(), "1".into()),
            ("action".into(), "added".into()),
        ]
    );
    // Short: what it holds.
    assert_eq!(decode_report(&layout, &p[..14]).len(), 2);
}

#[test]
fn config_fields_follow_the_declaration() {
    use florn_control::message::ParamSummary;
    let summary = ActorSummary {
        id: 1,
        name: "r1".into(),
        module: Some("dv".into()),
        class: "default".into(),
        ports: Vec::new(),
        config: vec![
            ("period".into(), "100ns".into()),
            ("note".into(), "x".into()),
        ],
    };
    let module = ModuleSummary {
        name: "dv".into(),
        source_sha256: String::new(),
        contract: String::new(),
        kind: "router".into(),
        description: None,
        port_kinds: Vec::new(),
        params: vec![
            ParamSummary {
                name: "period".into(),
                ty: "ticks".into(),
                required: false,
                default: Some("30s".into()),
                unit: Some("s".into()),
            },
            ParamSummary {
                name: "infinity".into(),
                ty: "u64".into(),
                required: false,
                default: Some("16".into()),
                unit: None,
            },
        ],
        channels: Vec::new(),
        needs: Vec::new(),
        tables: Vec::new(),
    };
    let f = config_fields(&summary, Some(&module));
    let names: Vec<&str> = f.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["period", "infinity", "note"]);
    assert_eq!(f[0].committed, "100ns");
    assert_eq!(f[0].kind, Kind::Quantity);
    assert_eq!(f[1].committed, "16");
    assert_eq!(f[1].kind, Kind::Integer);
    assert_eq!(f[2].kind, Kind::Text);
}
