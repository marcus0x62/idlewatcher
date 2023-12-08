/*
 * Copyright (c) 2023 Marcus Butler
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */

use std::env;
use std::io::{ErrorKind};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::time::{Duration,SystemTime};
use std::thread::sleep;
use getopts::Options;
use wayrs_client::{Connection, global::GlobalsExt, EventCtx, IoMode};
use wayrs_utils::seats::*;
use wayrs_protocols::ext_idle_notify_v1::{
    ext_idle_notification_v1, ExtIdleNotificationV1, ExtIdleNotifierV1,
};
use utmp_rs::UtmpEntry;

#[link(name = "c")]
extern "C" {
    fn getuid() -> u32;
}

#[derive(Debug)]
struct State {
    idle: bool,
    seats: Seats,
}

impl SeatHandler for State {
    fn get_seats(&mut self) -> &mut Seats {
        &mut self.seats
    }
}

enum WaylandState {
    Enabled((Connection<State>, State)),
    Disabled
}

const DEFAULT_SLEEP_CMD: &str = "/usr/bin/systemctl";
const DEFAULT_SLEEP_ARGS: &str = "suspend";
const DEFAULT_IDLE_TIME: u64 = 3600;

fn main() {
    let mut idle_limit: u64 = DEFAULT_IDLE_TIME; // Time in seconds.
    let mut idle_cmd: String = String::from(DEFAULT_SLEEP_CMD);
    let mut idle_cmd_args: Vec<String> = vec![];
    let mut wayland_idle: bool = false;

    let args: Vec<String> = env::args().collect();
    let mut opts = Options::new();
    opts.optopt("t", "timeout", "Idle Timeout", "");
    opts.optopt("c", "command", "Idle Command", "");
    opts.optflag("h", "help", "Usage information");

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => { m }
        Err(f) => { panic!("{}", f.to_string()); }
    }; 

    if matches.opt_present("h") {
        eprintln!("Usage: idlewatcher -t timeout -c command");
        eprintln!("\t-t timeout\tAn integer specifying the idle timeout in seconds.  Default: 3600 seconds.");
        eprintln!("\t-c command\tThe command to execute when the idle timeout is reached.  Defaults to systemctl suspend.");
        return;
    }

    if matches.opt_present("t") {
        idle_limit = matches.opt_str("t").unwrap().parse().expect("Timeout not an integer"); 
    }

    if matches.opt_present("c") {
        match matches.opt_str("c") {
            Some(cmd) => {
                let split = cmd.split(' ').collect::<Vec<&str>>();

                idle_cmd = String::from(split[0]);
                for elem in split[1..].iter() {
                    idle_cmd_args.push(elem.to_string());
                }
            },
            _ => {}
        }
    } else {
        let split = DEFAULT_SLEEP_ARGS.split(' ').collect::<Vec<&str>>();
        for elem in split.iter() {
            idle_cmd_args.push(elem.to_string());
        }
    }

    println!("Timeout: {} Idle Command: {} {:?}", idle_limit, idle_cmd, idle_cmd_args);

    // Wayland setup
    let mut wayland: WaylandState = WaylandState::Disabled;
    
    loop {
        let mut most_active: u64 = u64::MAX;

        if let Ok(entries) = utmp_rs::parse_from_path("/var/run/utmp") {
            let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).expect("Error getting system time").as_secs();

            for entry in entries {
                match entry {
                    UtmpEntry::UserProcess{line,..} => {
                        let filename = format!("/dev/{}", line);
                        let atime = fs::metadata(filename.clone()).unwrap().atime();
                        let idle_time = now - atime as u64;
                        
                        if idle_time < most_active {
                            most_active = idle_time;
                        }
                    },
                    _ => {}
                }
            }
        }

        let mut wayland_error = false;

        match wayland {
            WaylandState::Enabled((ref mut conn, ref mut state)) => {
                let _ = conn.flush(IoMode::NonBlocking);
                match conn.recv_events(IoMode::NonBlocking) {
                    Err(e) => {
                        if e.kind() != ErrorKind::WouldBlock {
                            wayland_error = true;
                            println!("Unexpected error: {:?}", e);
                        }
                    },
                    _ => { }
                }

                conn.dispatch_events(state);
                wayland_idle = state.idle;
            },
            WaylandState::Disabled => {
                wayland_error = true;
            }
        }

        // You would think the most natural way to do this would be to call this in the error block above, but you can't do that
        // because wayland ends up being borrowed there.
        if wayland_error == true {
            wayland = wayland_connect(idle_limit);
        }
        
        if most_active > idle_limit && wayland_idle == true {
            if most_active > idle_limit {
                eprintln!("Exceeded idle time due to tty atime");
            }
            if wayland_idle == true {
                eprintln!("Exceeded idle time due to Wayland idle notification");
            }
            
            wayland_idle = false; // Reset Wayland idle timer.

            match std::process::Command::new(idle_cmd.clone())
                .args(idle_cmd_args.clone())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn() {
                    Ok(_) => {}
                    Err(e) => { eprintln!("Error running sleep command: {:?}", e); }
                }
        }
        
        sleep(Duration::from_secs(5));
    }
}

fn way_idle_cb(e: EventCtx<State, ExtIdleNotificationV1>) {
    println!("Pre event handling Wayland idle state: {}", e.state.idle);
    match e.event {
        ext_idle_notification_v1::Event::Idled => {
            e.state.idle = true;
        },

        ext_idle_notification_v1::Event::Resumed => {
            e.state.idle = false;
        },

        _ => {
            eprintln!("Unknown event type received in way_idle_cb: {:?}", e.event);
        }
    }
}

fn wayland_connect(idle_limit: u64) -> WaylandState {
    let mut conn: Connection<_>;
    let globals: Vec<wayrs_client::protocol::wl_registry::GlobalArgs>;

    match env::var("WAYLAND_DISPLAY") {
        Ok(_) => { },
        Err(_) => {
            // Attempt to locate and set the WAYLAND_DISPLAY environment variable.
            eprintln!("WAYLAND_DISPLAY is not set.  Attempting to identify...");

            let uid: u32;
            let mut found: bool = false;
            
            unsafe {
                uid = getuid();
            }
            
            for i in 1..10 {
                let file = format!("/var/run/user/{}/wayland-{}", uid, i);
                match fs::metadata(file.clone()) {
                    Ok(_) => {
                        println!("Found display in {}...Setting WAYLAND_DISPLAY", file);
                        env::set_var("WAYLAND_DISPLAY", format!("wayland-{}", i));
                        found = true;
                        break;
                    },
                    Err(e) => { dbg!(e); }
                }
            }
            if found == false {
                eprintln!("Unable to identify Wayland display.");
                return WaylandState::Disabled;
            }
        }
    }

    let res = Connection::connect_and_collect_globals();
    match res {
        Ok((c,g)) => {
            conn = c;
            globals = g;
        },
        Err(e) => {
            println!("Cannot connect to Wayland compositor: {:?}", e);
            return WaylandState::Disabled;
        }
    }

    let mut state = State {
        idle: false,
        seats: Seats::bind(&mut conn, &globals),
    };

    match conn.blocking_roundtrip() {
        Ok(_) => {},
        Err(e) => {
            eprintln!("Error calling blocking_roundtrip: {:?}", e);
            return WaylandState::Disabled;
        }
    }

    conn.dispatch_events(&mut state);

    let idle_notifier: ExtIdleNotifierV1;
    match globals.bind::<ExtIdleNotifierV1, _>(&mut conn, 1..=1) {
        Ok(notifier) => { idle_notifier = notifier; },
        Err(e) => {
            eprintln!("Cannot bind idle notifier to wayland conn: {:?}", e);
            return WaylandState::Disabled;
        }
    }

    for seat in state.get_seats().iter() {
        idle_notifier.get_idle_notification_with_cb(&mut conn, idle_limit as u32 * 1000, seat, way_idle_cb);
    }

    WaylandState::Enabled((conn, state))
}

fn print_type(x: i64) { dbg!(x); }
