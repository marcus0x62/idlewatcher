Idlewatcher will execute a command after a specified idle timeout period.  By default, after one hour, it will execute systemctl suspend,
if all logged-in ttys and Wayland are idle.  If Wayland isn't running, or if idlewatcher could not connect to it, it will only look at
logged-in ttys.  It uses the atime field of each tty to determine idleness, which is updated on user input, but not output from any
running programs.
