Idlewatcher will execute a command after a specified idle timeout period.  By default, after one hour, it will execute systemctl suspend,
if all logged-in ttys and Wayland are idle.  If Wayland isn't running, or if idlewatcher could not connect to it, it will only look at
logged-in ttys.  It uses the atime field of each tty to determine idleness, which is updated on user input, but not output from any
running programs.

It currently accepts two (optional) command line arguments to control its behavior: -t and -c.  -t specifies the timeout interval, and
-c specifies the command to run upon reaching the idle limit.  If you specify a custom -c command with arguments, be sure to enclose the
entire block with quotes, to ensure it is parsed correctly:
 
$ idlewatcher -c 'my_program_to_execute arg1 arg2 ...'

To build with rust, clone with repository and run 'cargo build --release' from the source directory.  The executable will be
target/release/idlewatcher.

You can use the included idlewatcher.service unit file to automatically launch idlewatcher as a user service with systemd.  Be sure to
edit the file to indicate where you copied the binary to, then execute:

$ systemctl --user --now enable idlewatcher.service

$ journalctl --user -u idlewatcher will show any error messages.
