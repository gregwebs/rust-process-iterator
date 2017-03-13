extern crate process_iterator;
use process_iterator::{process_as_reader, process_read_consumer, output, Output};

use std::fs::File;
use std::iter::{Iterator};
use std::io::prelude::*;

#[test]
fn sort_iterator() {
    let f = File::open("tests/files/unsorted.txt").unwrap();

    let mut child_stream =
            process_as_reader(Some(f), Output::Parent,
                 streams::sort_cmd(&vec![])
              ).expect("process iterator failure");

    let mut output = String::new();
    child_stream.stdout.read_to_string(&mut output).unwrap();

    let nums: Vec<i32> = output.lines().map(|line| line.parse().unwrap()).collect();
    assert_eq!(vec![1,2,4,8], nums);
    child_stream.wait().expect("did not exit 0");
}

#[test]
fn zip_consumer() {
    let f = File::open("tests/files/unsorted.txt").unwrap();
    let output_file = File::create("tests/files/output.gz").unwrap();

    let output_status =
            process_read_consumer(
                output().stderr(Output::Parent)
                        .stdout(Output::ToFd(output_file))
              ,  f
              ,  streams::gzipped_cmd()
              ).expect("process iterator failure");

    if !output_status.success() {
        panic!("non-zero exit code {:?}", output_status.code());
    }
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

    pub fn gzipped_cmd() -> (String, Vec<String>) {
        ("/bin/gzip".to_owned(), vec!["--no-name".to_owned()])
    }
}
