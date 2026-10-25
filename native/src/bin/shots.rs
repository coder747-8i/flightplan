//! Headless screenshots of the real UI, rendered offscreen with wgpu (development aid).
//!
//!     cargo run --features shots --bin shots -- <script> <out-dir>
//!
//! The script uses the actions in `flightplan::ui::shot` plus:
//! click <label text>, type <text> (into the focused field), key <Enter|Escape|...>,
//! clickat <x> <y>, drag <x1> <y1> <x2> <y2>, scroll <x> <y> <dy>.

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use flightplan::app::App;
use std::time::{Duration, Instant};

fn pump(h: &mut Harness<'_, App>, min: Duration, max: Duration) {
    let start = Instant::now();
    loop {
        h.step();
        std::thread::sleep(Duration::from_millis(30));
        let t = start.elapsed();
        if (t >= min && flightplan::ui::shot::settled(h.state())) || t >= max {
            break;
        }
    }
    // let late repaints (textures uploaded this frame) land
    for _ in 0..3 {
        h.step();
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let script = std::fs::read_to_string(args.get(1).expect("script path")).expect("read script");
    let out = std::path::PathBuf::from(args.get(2).cloned().unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&out).ok();
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 920.0)).with_max_steps(1_000_000).wgpu().build_eframe(|cc| App::new(cc).expect("app"));
    pump(&mut h, Duration::from_millis(500), Duration::from_secs(10));
    for (a, arg, name) in flightplan::ui::shot::parse(&script) {
        let nums: Vec<f32> = arg.split_whitespace().filter_map(|v| v.parse().ok()).collect();
        match a.as_str() {
            "click" => match h.query_all_by_label_contains(&arg).next() {
                Some(n) => n.click(),
                None => eprintln!("no widget labelled {arg:?}"),
            },
            "type" => {
                h.event(egui::Event::Text(arg.clone()));
            }
            "key" => {
                let key = egui::Key::from_name(&arg).unwrap_or(egui::Key::Enter);
                h.key_press(key);
            }
            // ctrl <Key> / ctrlshift <Key>
            "ctrl" | "ctrlshift" => {
                let key = egui::Key::from_name(&arg).unwrap_or(egui::Key::Z);
                let m = if a == "ctrl" { egui::Modifiers::COMMAND } else { egui::Modifiers::COMMAND | egui::Modifiers::SHIFT };
                h.key_press_modifiers(m, key);
            }
            "clickat" if nums.len() == 2 => {
                let p = egui::pos2(nums[0], nums[1]);
                h.hover_at(p);
                h.step();
                h.drag_at(p);
                h.step();
                h.drop_at(p);
            }
            "drag" if nums.len() == 4 => {
                let (a, b) = (egui::pos2(nums[0], nums[1]), egui::pos2(nums[2], nums[3]));
                h.hover_at(a);
                h.step();
                h.drag_at(a);
                for i in 1..=12 {
                    h.step();
                    h.hover_at(a + (b - a) * (i as f32 / 12.0));
                }
                h.step();
                h.drop_at(b);
            }
            "sleep" if nums.len() == 1 => {
                let until = Instant::now() + Duration::from_secs_f32(nums[0]);
                while Instant::now() < until {
                    h.step();
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            "scroll" if nums.len() == 3 => {
                h.hover_at(egui::pos2(nums[0], nums[1]));
                h.event(egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, nums[2]), modifiers: egui::Modifiers::NONE, phase: egui::TouchPhase::Move });
            }
            _ => flightplan::ui::shot::act(h.state_mut(), &a, &arg, &out),
        }
        pump(&mut h, Duration::from_millis(if a == "click" || a == "type" || a == "key" { 400 } else { 2500 }), Duration::from_secs(20));
        if !name.is_empty() && name != "x" {
            match h.render() {
                Ok(img) => {
                    let p = out.join(format!("{name}.png"));
                    img.save(&p).ok();
                    eprintln!("shot {}", p.display());
                }
                Err(e) => eprintln!("render failed: {e}"),
            }
        }
    }
    h.state_mut().save_now();
}
