//! Check runtime-generated rendering fixtures, including the external font corpus.
#[cfg(feature = "bdf")]
#[path = "../tests/support/render_cases.rs"]
mod render_cases;
#[cfg(feature = "bdf")]
fn main() {
    let path = std::env::args().nth(1).expect("fixture path");
    let data = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let (good, errors) = render_cases::check(&data);
    println!("PASS {good} pixel/dimension outputs and {errors} error cases");
}
#[cfg(not(feature = "bdf"))]
fn main() {
    eprintln!("enable bdf");
}
