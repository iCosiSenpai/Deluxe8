//! Drives the launcher's Native-engine backend without the UI, against a scratch data folder:
//!
//!   DELUXE8_DATA_DIR=<scratch> [DELUXE8_NATIVE_DIR=<missing dir, to force the download>] \
//!     cargo run --example components_check -- <step>...
//!
//! Steps: `native` (download/unpack the Native engine), `dlss` (download NVIDIA's DLSS SDK + Vulkan headers, build and install
//! the plugin), `prepare <manifest.json> <prod.keys> <firmware>` (place the NSPs and build the game), `launch <seconds>` (start
//! the engine as the launcher does and print the fps samples it logs), `status`.
use std::sync::atomic::AtomicBool;

use deluxe8_launcher_lib::{components, native, paths::AppPaths, prepare::Manifest, settings::Settings};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let paths = AppPaths::resolve(None);
    std::fs::create_dir_all(&paths.logs).expect("data folder");
    deluxe8_launcher_lib::logs::init(paths.logs.join("components_check.log"));
    let cancel = AtomicBool::new(false);
    let mut report = |detail: String, progress: Option<f64>| {
        println!("  {:>5} {detail}", progress.map(|p| format!("{:.0}%", p * 100.0)).unwrap_or_default());
    };
    let mut i = 0;
    while i < args.len() {
        let started = std::time::Instant::now();
        match args[i].as_str() {
            "status" => println!("{:#?}", components::status(&paths)),
            "native" => {
                println!("== native engine");
                let dir = components::ensure_native_engine(&paths, &mut report, &cancel).expect("native engine");
                println!("  -> {}", dir.display());
            }
            "dlss" => {
                println!("== dlss");
                let root = paths.native_engine_dir().expect("a native engine first");
                let msg = components::install_dlss(&paths, &root, &mut report, &cancel).expect("dlss");
                println!("  -> {msg}");
            }
            "prepare" => {
                println!("== prepare");
                let manifest: Manifest = serde_json::from_str(&std::fs::read_to_string(&args[i + 1]).expect("manifest")).expect("manifest json");
                std::fs::create_dir_all(&paths.content).unwrap();
                std::fs::write(Manifest::path(&paths), serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
                let mut s = Settings::default();
                s.paths.prod_keys = args[i + 2].clone();
                s.paths.firmware = args[i + 3].clone();
                let step = |state: &str, detail: Option<String>, progress: Option<f64>| {
                    println!("  {state:>7} {:>5} {}", progress.map(|p| format!("{:.0}%", p * 100.0)).unwrap_or_default(), detail.unwrap_or_default());
                };
                native::prepare(&step, &paths, &s, &cancel).expect("native prepare");
                i += 3;
            }
            "launch" => {
                let seconds: u64 = args[i + 1].parse().expect("seconds");
                i += 1;
                println!("== launch for {seconds}s");
                let manifest = Manifest::load(&paths).expect("prepared manifest");
                let mut s = Settings::default();
                s.display.fullscreen = false;
                s.display.vsync = std::env::var("D8_TEST_VSYNC").unwrap_or_else(|_| "fifo".into());
                s.image.upscaler = std::env::var("D8_TEST_UPSCALER").unwrap_or_else(|_| "dlss".into());
                s.image.render_scale = std::env::var("D8_TEST_SCALE").ok().and_then(|v| v.parse().ok()).unwrap_or(67);
                s.smoothness.frame_gen = std::env::var("D8_TEST_FRAMEGEN").unwrap_or_else(|_| "dlss".into());
                let (cmd, log) = native::command(&paths, &s, &manifest).expect("command");
                let mut cmd = cmd.into_std();
                let mut child = cmd.spawn().expect("spawn");
                std::thread::sleep(std::time::Duration::from_secs(seconds));
                let _ = child.kill();
                let _ = child.wait();
                let text = std::fs::read_to_string(&log).unwrap_or_default();
                let fps: Vec<f64> = text
                    .lines()
                    .filter_map(|l| l.split("game_fps=").nth(1)?.split_whitespace().next()?.parse().ok())
                    .collect();
                for needle in ["upscaler '", "frame generation '", "[dlss]"] {
                    if let Some(line) = text.lines().find(|l| l.contains(needle)) {
                        println!("  {}", line.split(": ").last().unwrap_or(line));
                    }
                }
                let tail: Vec<String> = fps.iter().rev().take(10).rev().map(|f| format!("{f:.1}")).collect();
                println!("  {} fps samples, last: {}", fps.len(), tail.join(" "));
            }
            other => panic!("unknown step {other}"),
        }
        println!("  ({:.1}s)", started.elapsed().as_secs_f64());
        i += 1;
    }
}
