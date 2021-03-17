use std::vec::Vec;
use std::{env, process};
use timetracker::utils::{FileProxy, NamedFile};
use timetracker::{get_activities_file_name, get_configfile_name, get_logfile_name};

fn main() {
    let now = chrono::Local::now();
    let args: Vec<String> = env::args().collect();
    let result = timetracker::run(
        &args,
        &now,
        &NamedFile::new(get_configfile_name()),
        &NamedFile::new(get_logfile_name(&now.date().naive_local())),
        &NamedFile::new(get_activities_file_name()),
    );
    let retval = match result {
        Ok(n) => n,
        Err(e) => {
            eprintln!(
                "{}: {}",
                match env::current_exe() {
                    Err(_) => "?".to_string(),
                    Ok(path) => path
                        .components()
                        .last()
                        .map(|c| c.as_os_str().to_string_lossy().to_string())
                        .unwrap_or_else(|| "?".to_string()),
                },
                e
            );
            2
        }
    };
    process::exit(retval);
}
