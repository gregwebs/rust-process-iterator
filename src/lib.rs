use std::io;
use std::io::prelude::*;
use std::io::{BufReader};
use std::fs::File;
use std::process::{ChildStdout, Command, Stdio, ExitStatus};
use std::path::Path;
use std::thread;
use std::sync::Mutex;
use std::ops::DerefMut;


pub enum Output {
    Parent,
    Ignore,
    FailOnOutput,
    LogToFile(Box<Path>),
}

pub fn output() -> DealWithOutput {
    DealWithOutput { stdout: None, stderr: Output::Parent }
}

impl DealWithOutput {
    pub fn stderr(&mut self, stderr: Output) -> &mut Self {
        self.stderr = stderr;
        self
    }

    pub fn stdout(&mut self, stdout: Output) -> &mut Self {
        self.stdout = Some(stdout);
        self
    }
}

pub struct DealWithOutput { stderr: Output, stdout: Option<Output> }


// Feed the input as stdin to the process and wait for the process to finish
pub fn process_as_consumer<R>(deal_with: &mut DealWithOutput, mut input: R, cmd_args: (String, Vec<String>)) -> io::Result<ExitStatus>
    where R: Read
{
    let mut cmd = build_command(cmd_args);

    // being a consumer means there is no stdout

    setup_stderr(&deal_with.stderr, &mut cmd);

    let mut process = cmd.spawn()?;
    let mut stdin = process.stdin.take().expect("impossible! no stdin");

    output_optional_handle(&deal_with.stderr, &mut process.stderr)?;
    match deal_with.stdout {
        None => {}
        Some(ref stdout) => {
            output_optional_handle(&stdout, &mut process.stderr)?;
        }
    }

    io::copy(&mut input, &mut stdin)
      .expect("error writing stdin");

    let status = process.wait()?;
    Ok(status)
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
pub fn process_as_iterator<R>(deal_with: &mut DealWithOutput, input_opt: Option<R>, cmd_args: (String, Vec<String>)) -> io::Result<BufReader<ChildStdout>>
    where R: Read + Send + 'static
{
    let mut cmd = build_command(cmd_args);

    // setup stdout
    cmd.stdout(Stdio::piped());
    if let Some(_) = input_opt {
        cmd.stdin(Stdio::piped());
    }

    setup_stderr(&deal_with.stderr, &mut cmd);

    let mut process = cmd.spawn()?;
    let stdout = process.stdout.take().expect("impossible! no stdout");
    let mut stdin = process.stdin.take().expect("impossible! no stdin");

    output_optional_handle(&deal_with.stderr, &mut process.stderr)?;

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


fn setup_stderr(deal_with_stderr: &Output, cmd: &mut Command) -> () {
    // setup stderr
    match deal_with_stderr {
      &Output::Parent => {}
      &Output::Ignore => {
          // connecting to /dev/null would be better, but stderr should be small anyways
          cmd.stderr(Stdio::piped());
      }
      &Output::FailOnOutput => { cmd.stderr(Stdio::piped()); }
      &Output::LogToFile(_) => { cmd.stderr(Stdio::piped()); }
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


fn output_optional_handle<R: Read + Send + 'static>(deal_with_output: &Output, opt_handle: &mut Option<R>) -> io::Result<()> {
    if let &Output::FailOnOutput = deal_with_output {
        let handle = opt_handle.take().expect("impossible! no stderr");
        let _ = thread::spawn(move || {
            let mut stderr_output = Vec::new();
            let size = handle.take(1).read_to_end(&mut stderr_output).expect("error reading stdout");
            if size > 0 {
              panic!("got unexpected stderr");
            }
        });
    } else {
        if let &Output::LogToFile(ref path) = deal_with_output {
            let mut handle = opt_handle.take().expect("impossible! no stderr");
            let mut file = File::open(path)?;
            let _ = thread::spawn(move || {
                io::copy(&mut handle, &mut file)
                  .expect("error writing stderr to a log file");
            });
        }
    }

    Ok(())
}
