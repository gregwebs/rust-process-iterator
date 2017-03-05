#![allow(dead_code)]

extern crate process_iterator;
use process_iterator::{process_as_iterator, DealWithStderr};

use std::fs::File;
use std::io::prelude::*;

pub fn main(){
    let f = File::open("unsorted.txt").unwrap();

    let mut input_stream = 
            process_as_iterator(DealWithStderr::Parent,
                f
             ,  streams::sort_cmd(&vec![])
             ).expect("process iterator failure");

    let mut output = String::new();
    input_stream.read_to_string(&mut output).unwrap();
    print!("{}", &output);
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
