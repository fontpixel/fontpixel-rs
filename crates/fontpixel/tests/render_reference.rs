//! Checked-in synthetic Python rendering oracle.
#![cfg(feature = "bdf")]
#[path = "support/render_cases.rs"]
mod render_cases;
#[test]
fn synthetic_python_rendering_oracle() {
    let data = serde_json::from_str(include_str!("data/render_reference.json")).unwrap();
    let (good, errors) = render_cases::check(&data);
    assert!(good > 1000);
    assert!(errors > 10);
}
