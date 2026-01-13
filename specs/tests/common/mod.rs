use assert_cmd::Command;
use std::path::PathBuf;
use std::process::Command as StdCommand;

pub struct ParityTest {
    subcommand: String,
    args: Vec<String>,
}

pub struct CmdOutput {
    pub stdout: String,
    pub stderr: String,
    pub status: i32,
}

impl ParityTest {
    pub fn new(subcommand: &str) -> Self {
        Self {
            subcommand: subcommand.to_string(),
            args: Vec::new(),
        }
    }

    pub fn arg(mut self, arg: &str) -> Self {
        self.args.push(arg.to_string());
        self
    }

    pub fn run(self) {
        let root = get_workspace_root();
        
        println!("Running Parity Test for command: {} {:?}", self.subcommand, self.args);

        // 1. Run Legacy
        let legacy = run_legacy(&root, &self.subcommand, &self.args);
        
        // 2. Run Rust
        let rust = run_rust(&root, &self.subcommand, &self.args);

        // 3. Compare
        if rust.status != legacy.status {
            panic!("Exit Code Mismatch!\nLegacy: {}\nRust:   {}", legacy.status, rust.status);
        }

        compare_output("STDOUT", &rust.stdout, &legacy.stdout);
        compare_output("STDERR", &rust.stderr, &legacy.stderr);
    }
}

fn get_workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().expect("No parent dir").to_path_buf()
}

fn run_legacy(root: &PathBuf, subcommand: &str, args: &[String]) -> CmdOutput {
    let script = root.join("specs/spec_tool.sh");
    
    let mut cmd = StdCommand::new("bash");
    cmd.current_dir(root)
       .arg(script)
       .arg(subcommand)
       .args(args);
       
    let output = cmd.output().expect("Failed to run legacy script");
    
    CmdOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        status: output.status.code().unwrap_or(-1),
    }
}

fn run_rust(root: &PathBuf, subcommand: &str, args: &[String]) -> CmdOutput {
    let mut cmd = Command::cargo_bin("spec_tool").unwrap();
    cmd.current_dir(root)
       .arg(subcommand)
       .args(args);
       
    // assert_cmd::assert::Assert doesn't provide easy access to raw output if we want to continue, 
    // but cmd.output() works if converted to std::process::Command or using .output() from assert_cmd which returns Output.
    // Command::cargo_bin returns a process::Command wrapper from assert_cmd.
    
    // Using .assert() then .get_output() is fine, but we want to avoid asserting success immediately if we expect failure parity.
    let output = cmd.output().expect("Failed to run rust binary");

    CmdOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        status: output.status.code().unwrap_or(-1),
    }
}

fn compare_output(label: &str, rust: &str, legacy: &str) {
    if rust != legacy {
        println!("--- LEGACY {} ---", label);
        println!("{}", legacy);
        println!("--- RUST {} ---", label);
        println!("{}", rust);
        
        // Simple diff line count
        let r_lines: Vec<&str> = rust.lines().collect();
        let l_lines: Vec<&str> = legacy.lines().collect();
        println!("Line count: Legacy={}, Rust={}", l_lines.len(), r_lines.len());

        for (i, (l, r)) in l_lines.iter().zip(r_lines.iter()).enumerate() {
            if l != r {
                println!("Mismatch at line {}:", i + 1);
                println!("Legacy: {:?}", l);
                println!("Rust:   {:?}", r);
                break;
            }
        }

        panic!("{} Mismatch!", label);
    }
}
