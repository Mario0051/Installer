#![windows_subsystem = "windows"]

use std::env;
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().collect();

    let target_exe = if args.len() > 1 {
        args[1].clone()
    } else {
        "UmamusumePrettyDerby_Jpn.exe".to_string()
    };

    let target_args = if args.len() > 2 {
        args[2..].to_vec()
    } else {
        Vec::new()
    };

    if let Ok(mut child) = Command::new(&target_exe)
        .args(&target_args)
        .spawn()
    {
        let _ = child.wait();
    }
}