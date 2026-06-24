mod core;
mod syntax;
mod tui;
mod web;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (mut mode, mut file) = ("tui", None);
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-g" => mode = "gui",
            "-w" => mode = "web",
            "-h" | "--help" => {
                eprintln!("usage: editor [-g|-w] <file>");
                return;
            }
            s if !s.starts_with('-') => file = Some(s),
            _ => {}
        }
        i += 1;
    }
    let file = match file {
        Some(f) => f,
        None => {
            eprintln!("usage: editor [-g|-w] <file>");
            return;
        }
    };
    match mode {
        "tui" => tui::run(file).unwrap(),
        "gui" => {
            println!(
                "GUI mode: TUI provided instead (modern terminals = GUI)"
            );
            tui::run(file).unwrap()
        }
        "web" => web::run(file).unwrap(),
        _ => unreachable!(),
    }
}
