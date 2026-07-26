pub fn install_hooks() {
    color_eyre::install().unwrap_or_else(|_| {
        eprintln!("Failed to install color-eyre hooks");
    });
}
