//! Developer image operations. No host installer or block-device writes.
use linos::recovery::update::image::ImageStore;
use std::{io, path::Path};

fn run(args: &[String]) -> io::Result<()> {
    if let [command, a, b, data, esp] = args
        && command == "layout"
    {
        print!(
            "{}",
            linos::recovery::update::health::Layout::new(a, b, data, esp)?.encode()
        );
        return Ok(());
    }
    let store = match args {
        [command, root, payload, key, a, b] if command == "initialize" => {
            ImageStore::initialize(Path::new(root), Path::new(payload), Path::new(key), a, b)?
        }
        [command, root, payload, key] if command == "stage" => {
            let store = ImageStore::open(Path::new(root))?;
            store.stage(store.snapshot()?, Path::new(payload), Path::new(key))?;
            store
        }
        [command, root, key] if command == "activate" => {
            let store = ImageStore::open(Path::new(root))?;
            store.activate(store.snapshot()?, Path::new(key))?;
            store
        }
        [command, root] if command == "status" => ImageStore::open(Path::new(root))?,
        [command, root, destination, key, data, esp] if command == "export-guest" => {
            let store = ImageStore::open(Path::new(root))?;
            store.export_guest(Path::new(destination), Path::new(key), data, esp)?;
            store
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Usage: linos-update-image initialize ROOT PAYLOAD PUBLIC_KEY ROOT_A_PARTUUID ROOT_B_PARTUUID | stage ROOT PAYLOAD PUBLIC_KEY | activate ROOT PUBLIC_KEY | status ROOT | layout ROOT_A ROOT_B DATA ESP | export-guest ROOT EMPTY_DIRECTORY PUBLIC_KEY DATA ESP. Development image files only.",
            ));
        }
    };
    println!("Image update journal: {:?}", store.snapshot()?);
    Ok(())
}
fn main() {
    if let Err(error) = run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!("linos-update-image: {error}");
        std::process::exit(2);
    }
}
