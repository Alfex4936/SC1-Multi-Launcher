<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/rust-lang/www.rust-lang.org/master/static/images/rust-social-wide-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/rust-lang/www.rust-lang.org/master/static/images/rust-social-wide-light.svg">
    <img alt="The Rust Programming Language: A language empowering everyone to build reliable and efficient software"
         src="https://raw.githubusercontent.com/rust-lang/www.rust-lang.org/master/static/images/rust-social-wide-light.svg"
         width="30%">
  </picture>
</div>

# SC1-Multi-Launcher

![image](https://github.com/Alfex4936/chulbong-kr/assets/2356749/a1a61911-f325-414c-99f2-2eeae3c2c24c)

스타크래프트 1 게임을 여러 개 실행시킬 수 있는 앱

StarCraft I, Multi-Loader

This project is a Rust implementation using `windows-rs`.

I created this as a practice project to gain familiarity with `windows-rs` by porting the existing C++ version.

The original C++ version can be found at [sc_multiloader](https://github.com/somersby10ml/sc_multiloader/tree/main) by @somerby10ml

## Key Features

### Process Management
- **Launching Games**: Games are launched as separate processes, with their process identifiers (PIDs) and handles stored for management.
- **Killing Games**: Individual or all games can be terminated based on their PIDs and associated handles.

### Handle Management
- **Safe Handle Wrapping**: Utilizes `HandleWrapper` to ensure that process handles are managed safely, automatically closing handles when they are no longer needed.
- **Process Querying**: Leverages advanced Windows API calls to query running processes, obtain handles, and modify process properties.

## Implementation Details

### `GameManager` Structure
- Manages a list of game processes using a vector of tuples, each containing a PID and a handle.
- Provides methods to launch games, kill a specific game, or kill all managed games.

### Process and Handle Functions
- `launch_game`: Launches a game using specified path and stores its handle.
- `kill_a_game`: Terminates a game using its PID and closes its handle.
- `kill_all_games`: Terminates all tracked games and clears the list of handles.

### Handle Management with `HandleWrapper`
- Encapsulates process handles ensuring that they are closed properly using Rust's ownership and RAII principles.
- Provides methods to safely create and manage snapshots of system processes.

## Using `windows-rs`
This application heavily relies on `windows-rs` for:
- Creating processes with specific arguments and tracking their PIDs and handles.
- Querying and interacting with system processes to manage game instances effectively.
- Safely terminating processes and closing handles to avoid resource leaks.
- Prompts for administrative rights when needed, ensuring unrestricted access to critical system operations.
- Elevate the application to run with admin privileges, allowing for deeper system integrations and modifications.


## Safety and Concurrency
Uses Rust's async capabilities and safe concurrency models to manage multiple game processes simultaneously without risking data races or unsafe memory access.

## TODO

- [ ] leptos GUI
- [x] Process get
- [x] Process handle information
- [x] Kill process's handle