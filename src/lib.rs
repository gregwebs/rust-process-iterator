use std::io;
use std::io::prelude::*;
use std::io::{BufReader};
use std::fs::File;
use std::process::{Child, ChildStdout, Command, Stdio, ExitStatus};
use std::path::Path;
use std::thread;
use std::sync::Mutex;
use std::ops::DerefMut;


pub enum DealWithOutput {
    Parent,
    Ignore,
    FailOnOutput,
    LogToFile(Box<Path>),
}


// Feed the input as stdin to the process and wait for the process to finish
pub fn process_as_consumer<R>(deal_with_stderr: DealWithOutput, mut input: R, cmd_args: (String, Vec<String>)) -> io::Result<ExitStatus>
    where R: Read
{
    let mut cmd = build_command(cmd_args);

    // being a consumer means there is no stdout

    setup_stderr(&deal_with_stderr, &mut cmd);

    let mut process = cmd.spawn()?;
    let mut stdin = process.stdin.take().expect("impossible! no stdin");

    output_stderr(&deal_with_stderr, &mut process)?;

    io::copy(&mut input, &mut stdin)
      .expect("error writing stdin");

    let status = process.wait()?;
    Ok(status)
}


fn setup_stderr(deal_with_stderr: &DealWithOutput, cmd: &mut Command) -> () {
    // setup stderr
    match deal_with_stderr {
      &DealWithOutput::Parent => {}
      &DealWithOutput::Ignore => {
          // connecting to /dev/null would be better, but stderr should be small anyways
          cmd.stderr(Stdio::piped());
      }
      &DealWithOutput::FailOnOutput => { cmd.stderr(Stdio::piped()); }
      &DealWithOutput::LogToFile(_) => { cmd.stderr(Stdio::piped()); }
    }
}


// Build up the command from the arguments
fn build_command(cmd_args: (String, Vec<String>)) -> Command {
    // Build up the command from the arguments
    let (exe, args) = cmd_args;
    let mut cmd = Command::new(exe);
    for arg in args { cmd.arg(arg); }
    return cmd
}


fn output_stderr(deal_with_stderr: &DealWithOutput, process: &mut Child) -> io::Result<()> {
    if let &DealWithOutput::FailOnOutput = deal_with_stderr {
        let stderr = process.stderr.take().expect("impossible! no stderr");
        let _ = thread::spawn(move || {
            let mut stderr_output = Vec::new();
            let size = stderr.take(1).read_to_end(&mut stderr_output).expect("error reading stdout");
            if size > 0 {
              panic!("got unexpected stderr");
            }
        });
    } else {
        if let &DealWithOutput::LogToFile(ref path) = deal_with_stderr {
            let mut stderr = process.stderr.take().expect("impossible! no stderr");
            let mut file = File::open(path)?;
            let _ = thread::spawn(move || {
                io::copy(&mut stderr, &mut file)
                  .expect("error writing stderr to a log file");
            });
        }
    }

    Ok(())
}

// Provide an iterator interface to a process
//
// Start a process for the given exe and args.
// Return a buffer or the error encountered when starting the process
//
// Feed input (if given) to its stdin (in a separate thread)
// Return a reader of the stdout
//
// Check stder, and wiat for the exit code on separate threads
//
// If any of the threads have failures, including if the proces has a non-zero exit code,
// panic in the that separate thread.
// TODO: the caller should be signalled about the error
pub fn process_as_iterator<R>(deal_with_stderr: DealWithOutput, input_opt: Option<R>, cmd_args: (String, Vec<String>)) -> io::Result<BufReader<ChildStdout>>
    where R: Read + Send + 'static
{
    let mut cmd = build_command(cmd_args);

    // setup stdout
    cmd.stdout(Stdio::piped());
    if let Some(_) = input_opt {
        cmd.stdin(Stdio::piped());
    }

    setup_stderr(&deal_with_stderr, &mut cmd);

    let mut process = cmd.spawn()?;
    let stdout = process.stdout.take().expect("impossible! no stdout");
    let mut stdin = process.stdin.take().expect("impossible! no stdin");

    output_stderr(&deal_with_stderr, &mut process)?;

    // feed input to stdin
    if let Some(input) = input_opt {
        let input_mutex = Mutex::new(input);
        thread::spawn(move || {
            let mut inp = input_mutex.lock().unwrap();
            io::copy(inp.deref_mut(), &mut stdin)
              .expect("error writing stdin");
        });
    }

    // wait for the process to exit successfully
    thread::spawn(move || {
        // assert that the process exits successfully
        let result = process.wait()
              .expect("error writing for the process to stop");
        if !result.success() {
            panic!("non-zero exit code {:?}", result.code());
        }
    });

    Ok(BufReader::new(stdout))
}
