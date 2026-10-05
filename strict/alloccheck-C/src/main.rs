//! Cross-check the variants ledger against the allocator.
//!
//! A counting global allocator records the requested bytes live on the test
//! thread while planning and export run under their smallest accepted ledger.
//! If every allocation is admitted before it happens, the measured peak can
//! never exceed that ledger total. Allocator overhead is not counted, matching
//! the ledger's documented scope. `--tong` adds the full Tong Large regional
//! square/dot TTF/OTF exports read from the local validation inputs.
mod fixture;
use fontpixel::{
    Result, Variants,
    outline::Shape,
    variants::{self, DotReuse, Options, OutlineFlavor},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

struct Counting;

thread_local! {
    static TRACKING: Cell<bool> = const { Cell::new(false) };
    static LIVE: Cell<isize> = const { Cell::new(0) };
    static PEAK: Cell<isize> = const { Cell::new(0) };
}

fn record(delta: isize) {
    if TRACKING.with(Cell::get) {
        let live = LIVE.with(|l| {
            l.set(l.get() + delta);
            l.get()
        });
        PEAK.with(|p| p.set(p.get().max(live)));
    }
}

// SAFETY: every call forwards to the system allocator unchanged; the counters
// are const-initialized thread-local cells that never allocate or drop.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded with the caller's layout.
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            record(layout.size() as isize);
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded with the caller's layout.
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            record(layout.size() as isize);
        }
        p
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded with the caller's pointer and layout.
        unsafe { System.dealloc(ptr, layout) };
        record(-(layout.size() as isize));
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // Count the new block while the old one is live, as a copy would.
        record(new_size as isize);
        // SAFETY: forwarded with the caller's pointer, layout and size.
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if p.is_null() {
            record(-(new_size as isize));
        } else {
            record(-(layout.size() as isize));
        }
        p
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Peak requested bytes live on this thread while `run` executes, including
/// its returned value.
fn measured<T>(run: impl FnOnce() -> T) -> (T, usize) {
    LIVE.with(|l| l.set(0));
    PEAK.with(|p| p.set(0));
    TRACKING.with(|t| t.set(true));
    let value = run();
    TRACKING.with(|t| t.set(false));
    (value, PEAK.with(Cell::get).max(0) as usize)
}

/// Smallest accepted ledger total.
fn minimal(run: impl Fn(usize) -> Result<()>) -> usize {
    let (mut lo, mut hi) = (0usize, 2048 * 1024 * 1024);
    run(hi).unwrap();
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if run(mid).is_ok() {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

fn options(total: usize, edit: &dyn Fn(&mut Options)) -> Options {
    let mut o = Options::default();
    o.limits.sfnt.resources.max_total_bytes = total;
    o.gray_threshold = Some(128);
    edit(&mut o);
    o
}

fn check(v: &Variants, font: Option<&fontpixel::Font>, tong: bool) {
    let edits: Vec<(&str, Box<dyn Fn(&mut Options)>)> = vec![
        ("square ttf", Box::new(|_| {})),
        ("square otf", Box::new(|o| o.flavor = OutlineFlavor::Cff)),
        (
            "separate ttf",
            Box::new(|o| {
                o.shape = Shape::SquareSeparate;
                o.dot_size = 0.8;
            }),
        ),
        (
            "dot literal",
            Box::new(|o| {
                o.shape = Shape::Dot;
                o.dot_size = 0.9;
                o.dot_reuse = DotReuse::Literal;
            }),
        ),
        (
            "dot patterns",
            Box::new(|o| {
                o.shape = Shape::Dot;
                o.dot_size = 0.9;
            }),
        ),
        (
            "dot otf",
            Box::new(|o| {
                o.shape = Shape::Dot;
                o.dot_size = 0.9;
                o.flavor = OutlineFlavor::Cff;
            }),
        ),
        (
            "embedded ttf",
            Box::new(|o| {
                o.embed_bitmaps = true;
            }),
        ),
    ];
    for (name, edit) in &edits {
        if tong && (*name == "separate ttf" || *name == "dot literal" || *name == "embedded ttf") {
            continue;
        }
        let total = minimal(|t| variants::encode(v, &options(t, edit)).map(drop));
        let (result, peak) = measured(|| variants::encode(v, &options(total, edit)));
        result.unwrap();
        println!(
            "{name}: heap peak {peak} <= ledger {total}: {}",
            peak <= total
        );
        assert!(peak <= total, "{name}: heap {peak} > ledger {total}");
        let Some(font) = font else { continue };

        let total = minimal(|t| variants::encode_single(&font, &options(t, edit)).map(drop));
        let (result, peak) = measured(|| variants::encode_single(&font, &options(total, edit)));
        result.unwrap();
        println!(
            "{name} single: heap peak {peak} <= ledger {total}: {}",
            peak <= total
        );
        assert!(peak <= total, "{name} single: heap {peak} > ledger {total}");
    }
    let total = minimal(|t| v.plan(&options(t, &|_| {}).limits).map(drop));
    let (plan, peak) = measured(|| v.plan(&options(total, &|_| {}).limits));
    plan.unwrap();
    println!(
        "plan: heap peak {peak} <= ledger {total}: {}",
        peak <= total
    );
    assert!(peak <= total, "plan: heap {peak} > ledger {total}");
    // A prepared encode allocates only within its own allowance.
    let total = minimal(|t| {
        let o = options(t, &|_| {});
        v.prepare(&o.limits)?.encode(&o).map(drop)
    });
    let o = options(total, &|_| {});
    let prepared = v.prepare(&o.limits).unwrap();
    let (result, peak) = measured(|| prepared.encode(&o));
    result.unwrap();
    println!(
        "prepared encode: heap peak {peak} <= allowance {}: {}",
        prepared.encode_allowance(),
        peak <= prepared.encode_allowance()
    );
    assert!(
        peak <= prepared.encode_allowance(),
        "prepared encode: heap {peak} > allowance {}",
        prepared.encode_allowance()
    );
}

fn tong() -> Variants {
    let dir = "/home/chen/githubprojects/fontpixel/validation-inputs/phase2/bdf";
    let read = |region: &str| {
        let bytes = std::fs::read(format!("{dir}/TongPixelSans14Large-{region}.bdf")).unwrap();
        let options = fontpixel::ReadOptions {
            format: Some("bdf".into()),
            ..Default::default()
        };
        fontpixel::read(&bytes, &options)
            .unwrap()
            .fonts
            .into_single()
            .unwrap()
    };
    let mut v = Variants::new(read("SC"));
    let tc = read("TC");
    v.add_language("zh-Hant", tc.clone()).unwrap();
    v.add_language("zh-HK", tc).unwrap();
    v.add_language("ja", read("JP")).unwrap();
    v.add_language("ko", read("KR")).unwrap();
    v.scripts.push(fontpixel::variants::ScriptVariant {
        scripts: vec![*b"latn", *b"grek", *b"cyrl"],
        font: read("Latin"),
    });
    v
}

fn main() {
    println!("== fixture");
    check(&fixture::variants(), Some(&fixture::single()), false);
    if std::env::args().any(|a| a == "--tong") {
        println!("== full Tong Large regional");
        check(&tong(), None, true);
    }
}
