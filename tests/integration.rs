extern crate process_iterator;
use process_iterator::{process_as_iterator, output, Output};

use std::fs::File;
use std::iter::{Iterator};
use std::io::prelude::*;

#[test]
fn hello_world() {
    let f = File::open("tests/unsorted.txt").unwrap();

    // let some_in: Option<File> = None;
    let mut input_stream = 
            process_as_iterator(output().stderr(Output::Parent),
                 Some(f)
              ,  streams::sort_cmd(&vec![])
              ).expect("process iterator failure");

    let mut output = String::new();
    input_stream.read_to_string(&mut output).unwrap();

    let nums: Vec<i32> = output.lines().map(|line| line.parse().unwrap()).collect();
    assert_eq!(vec![1,2,4,8], nums);
}

mod streams {
    pub fn sort_cmd(columns: &Vec<(i32, &str)>) -> (String, Vec<String>) {
        let mut cmd: Vec<String> = Vec::new();
        let cmd_str = vec!["--stable", "--field-separator", "\t", "--parallel", "4"];
        for str in cmd_str { cmd.push(str.to_owned()) }
        for &(n, order) in columns {
            cmd.push(format!("--key={0},{0}{1}", n + 1, order));
        }
        ("sort".to_owned(), cmd)
    }
}
