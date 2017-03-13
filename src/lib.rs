#[macro_use]
extern crate log;

use std::io;
use std::io::prelude::*;
use std::io::{BufReader};
use std::fs::File;
use std::process::{ChildStdout, Command, Stdio, ExitStatus};
use std::sync::Mutex;
use std::ops::DerefMut;

use std::os::unix::io::FromRawFd;
use std::os::unix::io::AsRawFd;
use std::sync::mpsc;


pub enum Output {
    Parent,
    Ignore,
    // only works on Unix, the most efficient
    ToFd(File),
    /* The below would require additional error handling mechanisms
    // Write to this file in a background thread
    ToPath(PathBuf),
    // Check for any output in a background thread
    // Useful to assert that stderr has no output
    FailOnOutput,
    */
}

/*
pub fn output_to_path(path: &str) -> Output {
    Output::ToPath(PathBuf::from(path))
}
*/

pub struct DealWithOutput { stderr: Output, stdout: Output }

pub fn output() -> DealWithOutput {
    DealWithOutput { stdout: Output::Parent, stderr: Output::Parent }
}

impl DealWithOutput {
    pub fn stderr(&mut self, stderr: Output) -> &mut Self {
        self.stderr = stderr;
        self
    }

    pub fn stdout(&mut self, stdout: Output) -> &mut Self {
        self.stdout = stdout;
        self
    }
}


// Use a process as a consumer of a Read interface.
//
// Start a process for the given exe and args.
// Handle stderr and stdout in a non-blocking way according to the given DealWithOutput
//
// Feed the input as stdin to the process and then wait for the process to finish
// Return the result of the process.
pub fn process_read_consumer<R: Read>(
    deal_with: &mut DealWithOutput,
    mut input: R,
    cmd_args: (String, Vec<String>))
    -> io::Result<ExitStatus>
{
    let mut cmd = build_command(cmd_args);

    cmd.stdin(Stdio::piped());
    setup_stderr(&deal_with.stderr, &mut cmd)?;
    setup_stdout(&deal_with.stdout, &mut cmd)?;

    let mut process = cmd.spawn()?;

    // Introducing this scope will drop stdin
    // That will close the handle so the process can terminate
    {
        let mut stdin = process.stdin.take().expect("impossible! no stdin");

        // output_optional_handle(&deal_with.stderr, &mut process.stderr)?;
        // output_optional_handle(&stdout, &mut process.stdout)?;
        io::copy(&mut input, &mut stdin)?;
    }

    let status = process.wait()?;
    Ok(status)
}

pub struct ProcessReaderArgs<R: Read + Send + 'static> {
    stderr: Output,
    stdin: Option<R>,
}

pub fn process_reader_args<R: Read + Send + 'static>() -> ProcessReaderArgs<R> {
    ProcessReaderArgs { stderr: Output::Parent, stdin: None }
}

impl <R: Read + Send + 'static> ProcessReaderArgs<R> {
    pub fn stderr(&mut self, stderr: Output) -> &mut Self {
        self.stderr = stderr;
        self
    }
    pub fn stdin(&mut self, stdin: R) -> &mut Self {
        self.stdin = Some(stdin);
        self
    }
    pub fn default(&mut self) -> &mut Self {
        self
    }
}

// Stream data through a process using readers
//
// Start a process for the given exe and args.
// Return a buffer of stdout or the error encountered when starting the process
//
// Feed input (if given) to its stdin (in a separate thread)
//
// Wait for the exit code on a separate thread
// Handle stderr in a non-blocking way according to the stderr option
//
// If any of the threads have failures, including if the process has a non-zero exit code,
// that will be reflected in `ChildStream.wait`
pub fn process_as_reader<R>(
	args: &mut ProcessReaderArgs<R>,
	cmd_args: (String, Vec<String>)) -> io::Result<ChildStream>
    where R: Read + Send + 'static,
{
    let mut cmd = build_command(cmd_args);
    let stdin = args.stdin.take();

    // setup stdout
    cmd.stdout(Stdio::piped());
    if let Some(_) = stdin {
        cmd.stdin(Stdio::piped());
    }

    setup_stderr(&args.stderr, &mut cmd)?;

    let mut process = cmd.spawn()?;
    let stdout = process.stdout.take().expect("impossible! no stdout");

    // output_optional_handle(&args.stderr, &mut process.stderr)?;

    let (send_result, receiver) = mpsc::channel();
    // feed input to stdin
    if let Some(input) = stdin {
        let mut stdin = process.stdin.take().expect("impossible! no stdin");
        let input_mutex = Mutex::new(input);
        let sender = send_result.clone();
        let done_stdin = move |result| {
            match result {
                Err(err) => {
                    send_or_log_result(sender, Err(ProcessAsyncError::StdinError(err)))
                }
                Ok(Err(err)) => {
                    send_or_log_result(sender, Err(ProcessAsyncError::StdinError(err)))
                }
                // process wait returns the Ok
                Ok(Ok(_)) => {}
            }
        };
        concurrent::spawn_catch_panic(done_stdin, move || {
            // TODO: convert error types
            let mut inp = input_mutex.lock().expect("error locking stdin");
            let _ = io::copy(inp.deref_mut(), &mut stdin)?;
            Ok(())
        });
    }

    // wait for the process to exit successfully
    let sender = send_result.clone();
    let done_wait = move |result| {
        match result {
            Err(err) => {
                send_or_log_result(sender,
                    Err(ProcessAsyncError::WaitError(err)))
            }
            Ok(Err(err)) => {
                send_or_log_result(sender,
                    Err(ProcessAsyncError::WaitError(err)))
            }
            Ok(Ok(status)) => {
                send_or_log_result(sender, Ok(status));
            }
        }
    };
    concurrent::spawn_catch_panic(done_wait, move || {
        // assert that the process exits successfully
        let status = process.wait()?;
        Ok(status)
    });

    Ok(ChildStream {
      stdout: BufReader::new(stdout),
      wait_result: FutureExitResult::new(receiver)
    })
}


#[derive(Debug)]
pub enum ProcessAsyncError {
    RecvError(mpsc::RecvError),
    WaitError(io::Error),
    StdinError(io::Error),
    ExitStatusError(Option<i32>),
    AlreadyResolvedError,
}

type ProecessAsyncResult = Result<ExitStatus, ProcessAsyncError>;

pub struct FutureExitResult {
    recv: mpsc::Receiver<ProecessAsyncResult>,
    // This future should only be resolved once
    already: bool,
}

impl FutureExitResult {
   fn new(receiver: mpsc::Receiver<ProecessAsyncResult>) -> Self {
      FutureExitResult { recv: receiver, already: false }
   }

   fn exit_status(&mut self) -> ProecessAsyncResult {
       if self.already { return Err(ProcessAsyncError::AlreadyResolvedError) }
       self.already = true;
       match self.recv.recv() {
           Err(err) => {
               Err(ProcessAsyncError::RecvError(err))
           }
           Ok(stream_status) => {
               stream_status
           }
       }
   }

   pub fn wait(&mut self) -> Result<Option<i32>, ProcessAsyncError> {
        let status = self.exit_status()?;
        if status.success() {
            Ok(status.code())
        } else {
            Err(ProcessAsyncError::ExitStatusError(status.code()))
        }
   }
}


pub struct ChildStream {
    pub stdout: BufReader<ChildStdout>,
    pub wait_result: FutureExitResult
}
impl ChildStream {
   // Wait for the result of the process.
   // This should only be called once.
   // The second time it will return AlreadyResolvedError
   pub fn wait(&mut self) -> Result<Option<i32>, ProcessAsyncError> {
       self.wait_result.wait()
   }
}


fn send_or_log_result<T>(sender: mpsc::Sender<Result<T, ProcessAsyncError>>, result: Result<T, ProcessAsyncError>){
    match sender.send(result) {
        Ok(_) => {}
        Err(err) => error!("error sending done message: {}", err),
    }
}


fn setup_stderr(deal_with_stderr: &Output, cmd: &mut Command) -> io::Result<()> {
    // setup stderr
    match deal_with_stderr {
      &Output::Parent => {}
      &Output::Ignore => {
          cmd.stderr(Stdio::null());
      }
      &Output::ToFd(ref file) => {
          unsafe {
            cmd.stderr(Stdio::from_raw_fd(file.as_raw_fd()));
          }
      }
      /*
      &Output::FailOnOutput => {
          cmd.stderr(Stdio::piped());
      }
      &Output::ToPath(_) => {
          cmd.stderr(Stdio::piped());
      }
      */
    }
    Ok(())
}


fn setup_stdout(deal_with_stdout: &Output, cmd: &mut Command) -> io::Result<()> {
    // setup stderr
    match deal_with_stdout {
      &Output::Parent => {}
      &Output::Ignore => {
          cmd.stdout(Stdio::null());
      }
      &Output::ToFd(ref file) => {
          unsafe {
            cmd.stdout(Stdio::from_raw_fd(file.as_raw_fd()));
          }
      }
      /*
      &Output::FailOnOutput => {
          cmd.stdout(Stdio::piped());
      }
      &Output::ToPath(_) => {
          cmd.stdout(Stdio::piped());
      }
      */
    }
    Ok(())
}


// Build up the command from the arguments
fn build_command(cmd_args: (String, Vec<String>)) -> Command {
    // Build up the command from the arguments
    let (exe, args) = cmd_args;
    let mut cmd = Command::new(exe);
    for arg in args { cmd.arg(arg); }
    return cmd
}


/*
fn output_optional_handle<R: Read + Send + 'static>(deal_with_output: &Output, opt_handle: &mut Option<R>) -> io::Result<()> {
    if let &Output::FailOnOutput = deal_with_output {
        let handle = opt_handle.take().expect("impossible! no output handle");
        let _ = thread::spawn(move || {
            let mut output = Vec::new();
            let size = handle.take(1).read_to_end(&mut output).expect("error reading output");
            if size > 0 {
              panic!("got unexpected output");
            }
        });
    } else {
        if let &Output::ToPath(ref path) = deal_with_output {
            let mut handle = opt_handle.take().expect("impossible! no output handle");
            let mut file = File::create(path)?;
            let _ = thread::spawn(move || {
                io::copy(&mut handle, &mut file)
                  .expect("error writing output to a file");
            });
        }
    }

    Ok(())
}
*/


mod concurrent {
    use std::io;
    use std::io::{Error, ErrorKind};
    use std::panic;
    use std::thread;
    use std::thread::JoinHandle;
    use std::any::Any;


    pub fn caught_panic_to_io_error(err: Box<Any + Send + 'static>) -> io::Error {
        let msg = match err.downcast_ref::<&'static str>() {
            Some(s) => *s,
            None => {
                match err.downcast_ref::<String>() {
                    Some(s) => &s[..],
                    None => "Box<Any>",
                }
            }
        };

        Error::new(ErrorKind::Other, msg)
    }


    // Spawn a thread and catch any panic unwinds.
    // Call a callback function with the result
    // where Err is the message of the panic and
    // Ok is the return of the function
    pub fn spawn_catch_panic<Function, Returned, Finished>(done: Finished, f: Function) -> JoinHandle<()>
        where Function: FnOnce() -> Returned,
              Function: Send + 'static,
              Function: panic::UnwindSafe,
              Finished: FnOnce(io::Result<Returned>) -> (),
              Finished: Send + 'static,
    {
        thread::spawn(move || {
            let result = panic::catch_unwind(move || { f() });
            match result {
                Err(err) => { done(Err(caught_panic_to_io_error(err))) }
                Ok(ok)   => { done(Ok(ok)) }
            }
        })
    }
}
