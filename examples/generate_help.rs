fn main() {
    let help = clap_markdown::help_markdown::<detail_cli::Cli>();
    print!("{help}");
}
