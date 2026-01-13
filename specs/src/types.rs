use std::path::PathBuf;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Contract {
    pub id: String,
    pub file_path: PathBuf,
    pub start_line: usize, // 1-based
    // end_line is implicit by next contract or file end, but better to store it
    pub end_line: usize, 
}

#[derive(Debug, Clone)]
pub struct SpecFile {
    pub path: PathBuf,
    pub filename: String,
    pub title: String,
    pub contracts: Vec<String>, // IDs defined here
}

#[derive(Debug, Clone)]
pub struct TestFunction {
    pub name: String,
    pub line: usize,
    pub end_line: usize,
    pub labels: Vec<String>, // spec_expect labels
    pub context: Vec<String>, // Doc comments and attributes before fn
}

#[derive(Debug, Clone)]
pub struct TestFile {
    pub path: PathBuf,
    pub filename: String,
    // Map Contract ID -> List of Test Functions that cover it
    pub covered_contracts: HashMap<String, Vec<TestFunction>>, 
}
