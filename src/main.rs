mod util;

use util::game::GameManager;
use util::reg::{async_registry_search, get_game_path};

#[tokio::main]
async fn main() {
    if !util::admin::is_admin() {
        println!("Not running as admin. Attempting to elevate...");
        if util::admin::run_as_admin() {
            println!("Please restart the application with admin privileges.");
            return; // Exit the current instance
        } else {
            println!("Failed to elevate privileges.");
            return;
        }
    }

    // let search_term = "StarCraft";
    // let value_name = "InstallLocation";
    // let matches = async_registry_search(HKEY_LOCAL_MACHINE, search_term, value_name).await;
    // for (key, path) in matches {
    //     println!("Key: {}, Path: {}", key, path);
    // }

    let game_manager = GameManager::new();

    // tokio::runtime::Handle::current().spawn(async move {
    //     if let Err(e) = unsafe { util::game::modify_processes() }.await {
    //         eprintln!("Failed to modify process: {:?}", e);
    //     }
    // });

    if let Some(game_path) = get_game_path(true) {
        for _ in 0..3 {
            game_manager.launch_game(game_path.clone()).await;
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await; // Delay between launches
        }

        println!("{}", game_path.to_str().unwrap());
    } else {
        println!("StarCraft not found.");
    }

    println!("Press Enter to continue...");
    let mut pause = String::new();
    std::io::stdin().read_line(&mut pause).unwrap();

    game_manager.kill_all_games().await;
}
