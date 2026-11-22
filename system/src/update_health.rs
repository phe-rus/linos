use std::io;
fn main() -> io::Result<()> {
    if std::env::args_os().len() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "linos-update-health accepts no overrides",
        ));
    }
    let outcome = linos::recovery::update::health::approve_running_boot()?;
    println!("Linos development update health: {outcome:?}");
    Ok(())
}
