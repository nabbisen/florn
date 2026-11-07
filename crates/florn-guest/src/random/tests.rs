use super::*;

const RECORDED: [u64; 6] = [
    7806831264735756412,
    5,
    164,
    4600568673609213140,
    4605322125149212083,
    13831055547662332364,
];
const CHOSEN: &str = "a";

struct Counter(u64);
impl Source for Counter {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }
}

#[test]
fn the_algorithms_are_fixed() {
    let mut r = Random::over(Counter(1));
    // Recorded once from this implementation: a change to any algorithm changes them.
    let recorded = [
        r.u64(),
        r.below(10),
        r.range(100, 200),
        r.f64().to_bits(),
        r.exponential(2.0).to_bits(),
        r.normal(0.0, 1.0).to_bits(),
    ];
    assert_eq!(recorded, RECORDED);
    assert_eq!(*r.choose(&["a", "b", "c"]), CHOSEN);
    let f = f64::from_bits(recorded[3]);
    assert!((0.0..1.0).contains(&f));
    // `below` is unbiased in distribution: every residue appears, none twice as often.
    let mut counts = [0u32; 7];
    for _ in 0..70_000 {
        counts[r.below(7) as usize] += 1;
    }
    assert!(
        counts.iter().all(|c| (9_000..11_000).contains(c)),
        "{counts:?}"
    );
}
