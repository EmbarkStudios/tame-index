#![allow(missing_docs)]

use tame_index::utils::flock;

fn main() {
    let mut args = std::env::args().skip(1);
    let kind = args.next().unwrap();
    let path = args.next().unwrap();

    let lo = flock::LockOptions::new(tame_index::Path::new(&path));

    let lo = match kind.as_str() {
        "shared" => lo.shared(),
        "exclusive" => lo.exclusive(false),
        _ => panic!("unknown lock kind '{kind}'"),
    };

    let _fl = lo.try_lock().expect("failed to acquire lock");
    {
        use std::io::Write;
        let mut stdout = std::io::stdout();
        stdout.write(&('🔒' as u32).to_le_bytes()).unwrap();
        stdout.flush().unwrap();
    }

    // If the test that spawned this process fails it won't reap this process, so
    // don't loop forever
    std::thread::sleep(std::time::Duration::from_secs(30));

    // Unnecessary, we shouldn't ever get here unless the test that called us failed
    std::process::exit(1);
}
