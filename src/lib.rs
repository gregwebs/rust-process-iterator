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
      DealWithStderr::Ignore => {
          // connecting to /dev/null would be better
          cmd.stderr(Stdio::piped());
      }
      DealWithStderr::FailOnOutput => { cmd.stderr(Stdio::piped()); }
      DealWithStderr::LogToFile(_) => { cmd.stderr(Stdio::piped()); }
    }

    let mut process = cmd.spawn()?;
    let stdout = process.stdout.take().expect("impossible! no stdout");
    let mut stdin = process.stdin.take().expect("impossible! no stdin");

    // deal with stderr
    if let DealWithStderr::FailOnOutput = deal_with_stderr {
        let stderr = process.stderr.take().expect("impossible! no stderr");
        let _ = thread::spawn(move || {
            let mut stderr_output = Vec::new();
            let size = stderr.take(1).read_to_end(&mut stderr_output).expect("error reading stdout");
            if size > 0 {
              panic!("got unexpected stderr");
            }
        });
    } else {
        if let DealWithStderr::LogToFile(path) = deal_with_stderr {
            let mut stderr = process.stderr.take().expect("impossible! no stderr");
            let mut file = File::open(path)?;
            let _ = thread::spawn(move || {
                io::copy(&mut stderr, &mut file)
                  .expect("error writing stderr to a log file");
            });
        }
    }

    // feed input to stdin and then wait for the process to exit successfully
    {
        let input_mutex = Mutex::new(input);
        thread::spawn(move || {
            let mut inp = input_mutex.lock().unwrap();
            io::copy(inp.deref_mut(), &mut stdin)
              .expect("error writing stdin");

            // assert that the process exits successfully
            let result = process.wait()
                  .expect("error writing for the process to stop");
            if !result.success() {
                panic!("non-zero exit code {:?}", result.code());
            }
        });
    }

    Ok(BufReader::new(stdout))
}
