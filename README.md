# rust-process-iterator

Stream data through an external process.
The stdlib interfaces to running commands and those from other packages do not readily support streaming.
This package fills the gap.

This package supplies 2 functions:

  * `process_read_consumer`
  * `process_as_reader`

`process_read_consumer` takes a `Read` as stdin and waits for the process to finish.
`process_as_reader` gives back a `Read` of stdout

One difficult aspect of streaming is error handling. `process_as_reader` additionally gives back a future to check for errors

Things in this package should look somewhat familiar to the `Child` data type that is getting wrapped.
Of notable addition is the `Output` type, in particular `Output::ToFd` which directs the command builder to direct output directly to a given file (which must have a valid descriptor).


# Development

You can encapsulate development of this library in a docker container.
There is a `./bin/cargo` script for invoking cargo in a container with the working directory
mounted.
