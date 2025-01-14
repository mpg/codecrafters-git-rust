use std::env;
use std::fs;

fn git_init() {
    fs::create_dir(".git").unwrap();
    fs::create_dir(".git/objects").unwrap();
    fs::create_dir(".git/refs").unwrap();
    fs::write(".git/HEAD", "ref: refs/heads/main\n").unwrap();
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args[1] == "init" {
        git_init();
        println!("Initialized git directory");
    } else {
        println!("Unknown command: {}", args[1]);
    }
}
