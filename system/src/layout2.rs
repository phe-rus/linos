fn main() {
    let result = match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [command] if command == "prepare-root" => linos::recovery::boot::prepare_root(),
        [command] if command == "mount-state" => linos::recovery::boot::mount_state(),
        [command] if command == "verify" => linos::recovery::boot::verify_running(),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Usage: linos-layout2 prepare-root|mount-state|verify",
        )),
    };
    if let Err(error) = result {
        eprintln!("Layout 2 boot refused: {error}");
        std::process::exit(2);
    }
}
