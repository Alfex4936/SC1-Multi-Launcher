use std::{io::Write, path::PathBuf};

use sclauncher::util::{
    admin::{is_admin, run_as_admin},
    game::GameManager,
    reg::{async_registry_search, get_game_path, set_game_path},
};

use clap::Parser;
use winconsole::console::{self};
use winreg::enums::HKEY_LOCAL_MACHINE;

use tokio::time::{sleep, Duration};

#[derive(Parser, Debug)]
#[command(
    name = "SC1 Mutli Loader",
    version = "1.0",
    author = "Seok Won Choi",
    about = "StarCraft 1 Multi launcher"
)]
struct Args {
    /// Performs an asynchronous search in the registry
    #[arg(short, long, action = clap::ArgAction::SetTrue)]
    async_registry_search: bool,

    /// Number of times to launch the game
    #[arg(short, long, default_value_t = 0)]
    num_launches: u32,

    /// 64bits or 32bits
    #[arg(short = 'b', long, default_value_t = false)]
    is_64bit: bool,
}

#[tokio::main]
async fn main() {
    if !is_admin() {
        println!("Not running as admin. Attempting to elevate...");
        if run_as_admin() {
            println!("Please restart the application with admin privileges.");
            return;
        } else {
            println!("Failed to elevate privileges.");
            return;
        }
    }
    console::set_title("SC1 Multi Launcher").unwrap();

    // ask user for input
    let mut args = Args::parse();
    if args.num_launches == 0 {
        args.num_launches = prompt_user_for_n();
    }

    // Setup and run the game management logic
    let game_manager = GameManager::new();
    let game_path = match get_game_path_or_search(&args).await {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Error getting game path: {}", e);
            return;
        }
    };

    // Launch the game the specified number of times
    launch_game_multiple_times(&game_manager, &game_path, args.num_launches).await;

    // Ensure games are killed before exiting
    println!("Press Enter to kill all games...");
    let mut pause = String::new();
    std::io::stdin().read_line(&mut pause).unwrap();

    game_manager.kill_all_games().await;
}

async fn get_game_path_or_search(args: &Args) -> Result<PathBuf, String> {
    // Try to get the game path directly
    let direct_path = get_game_path(args.is_64bit);

    if args.async_registry_search || direct_path.is_none() {
        println!("Attempting to locate StarCraft.exe...");

        // Perform the registry search if direct path is not found or if async search is requested
        let matches =
            async_registry_search(HKEY_LOCAL_MACHINE, "StarCraft", "InstallLocation").await;
        if matches.is_empty() && direct_path.is_none() {
            // If no matches and no direct path, prompt user for manual input
            return prompt_user_for_path();
        }

        // Decide subfolder based on architecture
        let exe_subfolder = if args.is_64bit { "x86_64" } else { "x86" };

        // Use the first found path from registry search if available
        matches.first().map_or_else(
            || direct_path.ok_or_else(|| "StarCraft not found.".to_string()),
            |(_, path)| {
                let _ = set_game_path(path); // try to set new path registry
                let mut game_path = PathBuf::from(path);
                game_path.push(exe_subfolder);
                game_path.push("StarCraft.exe");
                Ok(game_path)
            },
        )
    } else {
        // If direct path is found, return it
        direct_path.ok_or_else(|| "StarCraft not found.".to_string())
    }
}

// As default, it retuns 2 on errors
fn prompt_user_for_n() -> u32 {
    print!("How many StarCraft.exe?: ");
    std::io::stdout().flush().expect("Failed to flush stdout");

    let mut n = String::new();
    std::io::stdin()
        .read_line(&mut n)
        .map_err(|e| e.to_string())
        .expect("Failed to read line.");

    let trimmed_n = n.trim();
    if trimmed_n.is_empty() {
        return 2;
    }

    match trimmed_n.parse::<u32>() {
        Ok(num) => num,
        Err(_) => 2,
    }
}

fn prompt_user_for_path() -> Result<PathBuf, String> {
    println!("ex) D:\\Games\\StarCraft");
    print!("Please enter the full path to StarCraft.exe:");
    std::io::stdout().flush().expect("Failed to flush stdout");
    let mut path_input = String::new();
    std::io::stdin()
        .read_line(&mut path_input)
        .map_err(|e| e.to_string())?;

    let trimmed_path = path_input.trim();
    if trimmed_path.is_empty() {
        return Err("No path provided.".to_string());
    }

    let mut path = PathBuf::from(trimmed_path);

    // Check if the provided path is a direct path to 'StarCraft.exe'
    if path.ends_with("StarCraft.exe") {
        path.pop(); // Remove 'StarCraft.exe' from the path if it exists
    }

    // Check existence of the base path
    if !path.exists() {
        return Err("The provided base path does not exist.".to_string());
    }

    // Check for both 'x86' and 'x86_64' subdirectories with 'StarCraft.exe'
    let x86_path = path.join("x86\\StarCraft.exe");
    let x64_path = path.join("x86_64\\StarCraft.exe");

    if x86_path.exists() {
        return Ok(x86_path);
    } else if x64_path.exists() {
        return Ok(x64_path);
    } else {
        return Err(format!(
            "Neither '{}' nor '{}' exist.",
            x86_path.display(),
            x64_path.display()
        ));
    }
}

async fn launch_game_multiple_times(game_manager: &GameManager, path: &PathBuf, num_launches: u32) {
    for i in 0..num_launches {
        println!(" ~ Launching StarCraft.exe [{}]", i + 1);
        game_manager.launch_game(path.to_path_buf()).await;
        sleep(Duration::from_secs(1)).await;
    }
}
