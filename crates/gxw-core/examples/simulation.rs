//! Deterministic standalone Rust oracle for binding and distribution parity.
use gxw_core::*;
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path, sync::Arc};
#[derive(Deserialize)]
struct Config {
    period_ns: u64,
    initial_state: BTreeMap<String, bool>,
    inputs: Vec<BTreeMap<String, bool>>,
    run_scans: u64,
    watch: Vec<String>,
    #[serde(default)]
    phase_ns: u64,
    #[serde(default)]
    actions: Vec<Action>,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Action {
    Step { inputs: BTreeMap<String, bool> },
    Stop,
    Start,
    AdvanceStopped { elapsed_ns: u64 },
    Period { period_ns: u64 },
    Reset,
}
fn pairs(v: &BTreeMap<String, bool>) -> Vec<(String, bool)> {
    v.iter().map(|(k, v)| (k.clone(), *v)).collect()
}
fn run(
    format: &str,
    path: &str,
    index: usize,
    config: &Config,
) -> Result<serde_json::Value, GxwError> {
    let p = if format == "csv" {
        if index != 0 {
            return Err(GxwError::Format {
                context: "CSV program".into(),
                message: "index must be zero".into(),
            });
        }
        Arc::new(load_csv(
            Path::new(path),
            DeviceProfile::Fx,
            &ReadLimits::default(),
        )?)
    } else {
        let raw = Arc::new(load_path(Path::new(path), &ReadLimits::default())?);
        Arc::new(decode_program(raw, index, DeviceProfile::Fx)?)
    };
    let compiled = Arc::new(compile_program(p, SimulationProfile::Fx3g, 100_000)?);
    let mut s = Simulator::with_timer_phase(
        compiled.clone(),
        config.period_ns,
        &pairs(&config.initial_state),
        SimulationLimits::default(),
        config.phase_ns,
    )?;
    let mut action_snapshots = Vec::new();
    for action in &config.actions {
        let snapshot = match action {
            Action::Step { inputs } => {
                s.set_inputs(&pairs(inputs))?;
                s.step(true)?
            }
            Action::Stop => {
                s.stop();
                s.snapshot()
            }
            Action::Start => {
                s.start();
                s.snapshot()
            }
            Action::AdvanceStopped { elapsed_ns } => {
                s.advance_stopped(*elapsed_ns)?;
                s.snapshot()
            }
            Action::Period { period_ns } => {
                s.set_scan_period_ns(*period_ns)?;
                s.snapshot()
            }
            Action::Reset => {
                s.reset(&[])?;
                s.snapshot()
            }
        };
        action_snapshots.push(snapshot);
    }
    let mut snapshots = Vec::new();
    for inputs in &config.inputs {
        s.set_inputs(&pairs(inputs))?;
        snapshots.push(s.step(true)?);
    }
    let trace = s.run(config.run_scans, Some(&config.watch))?;
    s.reset(&pairs(&config.initial_state))?;
    Ok(
        serde_json::json!({"compiled":&*compiled,"snapshots":snapshots,"action_snapshots":action_snapshots,"trace":trace,"reset":s.snapshot()}),
    )
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 || !matches!(args[1].as_str(), "csv" | "gxw") {
        return Err("usage: simulation gxw|csv FILE INDEX CONFIG.json".into());
    }
    let config = serde_json::from_slice(&std::fs::read(&args[4])?)?;
    match run(&args[1], &args[2], args[3].parse()?, &config) {
        Ok(value) => println!("{}", serde_json::to_string(&value)?),
        Err(e) => {
            let value = if let GxwError::Simulation {
                ref code,
                instruction,
                ..
            } = e
            {
                serde_json::json!({"error":e.to_string(),"code":code,"instruction":instruction})
            } else {
                serde_json::json!({"error":e.to_string()})
            };
            println!("{value}");
            std::process::exit(3);
        }
    }
    Ok(())
}
