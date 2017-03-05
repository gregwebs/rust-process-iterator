#![allow(dead_code)]

use process::{process_as_iterator, DealWithStderr};
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

mod process {
    use std::io;
    use std::io::prelude::*;
    use std::io::{BufReader};
    use std::fs::File;
    use std::process::{ChildStdout, Command, Stdio};
    use std::path::Path;
    use std::thread;
    use std::sync::Mutex;
    use std::ops::DerefMut;


    pub enum DealWithStderr {
        Parent,
        Ignore,
        FailOnOutput,
        LogToFile(Box<Path>),
    }

    // Start a process for the given exe and args.
    // Feed the input to its stdin (in a separate thread)
    // Return a reader of the stdout
    //
    // Feeding stdin, checking stderr, and wiating for the exit code are done on separate threads
    // If any of these have failures, including if the proces has a non-zero exit code,
    // panic in the that separate thread.
    // TODO: the caller should be signalled about the error
    pub fn process_as_iterator<R>(deal_with_stderr: DealWithStderr, input: R, cmd_args: (String, Vec<String>)) -> io::Result<BufReader<ChildStdout>>
        where R: Read + Send + 'static
    {
        println!("{:?}", cmd_args);
        let (exe, args) = cmd_args;
        let mut cmd = Command::new(exe);
        for arg in args { cmd.arg(arg); }

        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        match deal_with_stderr {
          DealWithStderr::Parent => {}
          DealWithStderr::Ignore => {}
          DealWithStderr::FailOnOutput => { cmd.stderr(Stdio::piped()); }
          DealWithStderr::LogToFile(_) => { cmd.stderr(Stdio::piped()); }
        }

        let process = cmd.spawn()?;
        let stdout = process.stdout.expect("impossible! no stdout");
        let mut stdin = process.stdin.expect("impossible! no stdin");

        // feed input to stdin and then wait for the process to exit successfully
        {
            let input_mutex = Mutex::new(input);
            thread::spawn(move || {
                let mut inp = input_mutex.lock().unwrap();
                io::copy(inp.deref_mut(), &mut stdin)
                  .expect("error writing stdin");

                // assert that the process exits successfully
                /*
                let result = process.wait()
                      .expect("error writing for the process to stop");
                if !result.success() {
                    panic!("non-zero exit code {:?}", result.code());
                }
                */
            });
        }

        // deal with stderr
        if let DealWithStderr::FailOnOutput = deal_with_stderr {
            let stderr = process.stderr.expect("impossible! no stderr");
            let _ = thread::spawn(move || {
                let mut stderr_output = Vec::new();
                let size = stderr.take(1).read_to_end(&mut stderr_output).expect("error reading stdout");
                if size > 0 {
                  panic!("got unexpected stderr");
                }
            });
        } else {
            if let DealWithStderr::LogToFile(path) = deal_with_stderr {
                let mut stderr = process.stderr.expect("impossible! no stderr");
                let mut file = File::open(path)?;
                let _ = thread::spawn(move || {
                    io::copy(&mut stderr, &mut file)
                      .expect("error writing stderr to a log file");
                });
            }
        }

        Ok(BufReader::new(stdout))
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
}
