use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct Listener {
    pub port: u16,
    pub pid: u32,
    pub identity: Option<String>,
    pub process: String,
    pub addresses: Vec<String>,
    pub directory: Option<PathBuf>,
    pub project: Option<String>,
    pub executable: Option<PathBuf>,
    pub group: Group,
}

// Group identity uses paths, so projects with the same package name stay separate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Group {
    pub category: Category,
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    Projects,
    Apps,
    Unknown,
}

impl Default for Group {
    fn default() -> Self {
        Self {
            category: Category::Unknown,
            key: "unknown".into(),
            label: "Unidentified".into(),
        }
    }
}

// This function does blocking OS and filesystem work; call it on a background thread.
pub fn scan() -> Result<Vec<Listener>, String> {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-F0pcn"])
        .output()
        .map_err(|error| format!("Could not inspect listening ports: {error}"))?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        // lsof uses exit status 1 when no matching files were found.
        if output.status.code() != Some(1) || !message.is_empty() {
            return Err(format!("Could not inspect listening ports: {message}"));
        }
    }

    let mut listeners = parse_listeners(&String::from_utf8_lossy(&output.stdout));
    if listeners.is_empty() {
        return Ok(listeners);
    }

    let pids = listeners
        .iter()
        .map(|listener| listener.pid.to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(",");
    let mut metadata = BTreeMap::new();
    // A process may exit between these two queries. Missing paths are normal.
    if let Ok(output) = Command::new("/usr/sbin/lsof")
        .args(["-a", "-p", &pids, "-d", "cwd,txt", "-F0pfn"])
        .output()
    {
        metadata = parse_metadata(&String::from_utf8_lossy(&output.stdout));
    }

    let identities = process_identities(&pids)?;
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    for listener in &mut listeners {
        listener.identity = identities.get(&listener.pid).cloned();
        if let Some(info) = metadata.get(&listener.pid) {
            listener.directory = info.directory.clone();
            listener.executable = info.executable.clone();
        }
        listener.project = listener.directory.as_deref().and_then(project_name);
        listener.group = classify(listener, &home);
    }
    Ok(listeners)
}

// PID alone can be reused. Compare process start time and command before signalling.
fn process_identities(pids: &str) -> Result<BTreeMap<u32, String>, String> {
    let output = Command::new("/bin/ps")
        .args(["-p", pids, "-o", "pid=,lstart=,comm="])
        .output()
        .map_err(|e| format!("Could not identify processes: {e}"))?;
    let mut result = BTreeMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let line = line.trim();
        if let Some((pid, identity)) = line.split_once(char::is_whitespace)
            && let Ok(pid) = pid.parse()
        {
            result.insert(pid, identity.trim().to_owned());
        }
    }
    Ok(result)
}

pub fn stop(listeners: &[Listener], force: bool) -> Result<(), String> {
    let targets: BTreeMap<_, _> = listeners.iter().map(|row| (row.pid, row)).collect();
    if targets.is_empty() {
        return Err("No process selected".into());
    }
    let pids = targets
        .keys()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let current = process_identities(&pids)?;
    // Validate all targets before sending any signals.
    for (&pid, row) in &targets {
        if pid <= 1
            || pid == std::process::id()
            || row.identity.is_none()
            || current.get(&pid) != row.identity.as_ref()
        {
            return Err(format!(
                "PID {pid} has exited or changed. Refresh before trying again."
            ));
        }
    }
    for pid in targets.keys() {
        let output = Command::new("/bin/kill")
            .args([if force { "-KILL" } else { "-TERM" }, &pid.to_string()])
            .output()
            .map_err(|e| format!("Could not stop PID {pid}: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "Could not stop PID {pid}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }
    Ok(())
}

pub fn owner_app(row: &Listener) -> Option<PathBuf> {
    if row.group.key == "local" {
        return Some("/Applications/Local.app".into());
    }
    row.executable
        .as_ref()?
        .ancestors()
        .filter(|path| path.extension().is_some_and(|ext| ext == "app"))
        .last()
        .map(Path::to_path_buf)
}

pub fn open_owner(path: &Path) -> Result<(), String> {
    let output = Command::new("/usr/bin/open")
        .arg("-a")
        .arg(path)
        .output()
        .map_err(|e| format!("Could not open app: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

fn fields(output: &str) -> impl Iterator<Item = &str> {
    // Null-separated fields preserve spaces and newlines in names and paths.
    output
        .split('\0')
        .map(|field| field.trim_start_matches('\n'))
}

fn parse_listeners(output: &str) -> Vec<Listener> {
    let mut rows = BTreeMap::new();
    let mut pid = None;
    let mut process = String::new();
    for field in fields(output) {
        if let Some(value) = field.strip_prefix('p') {
            pid = value.parse::<u32>().ok();
            process.clear();
        } else if let Some(value) = field.strip_prefix('c') {
            process = value.to_owned();
        } else if let Some(address) = field.strip_prefix('n') {
            let Some(pid) = pid else { continue };
            let Some(port) = address
                .rsplit(':')
                .next()
                .and_then(|p| p.parse::<u16>().ok())
            else {
                continue;
            };
            // IPv4 and IPv6 sockets for the same process/port are one row.
            let row = rows.entry((port, pid)).or_insert_with(|| Listener {
                port,
                pid,
                identity: None,
                process: process.clone(),
                addresses: Vec::new(),
                directory: None,
                project: None,
                executable: None,
                group: Group::default(),
            });
            if !row.addresses.iter().any(|existing| existing == address) {
                row.addresses.push(address.to_owned());
            }
        }
    }
    rows.into_values().collect()
}

#[derive(Default)]
struct ProcessMetadata {
    directory: Option<PathBuf>,
    executable: Option<PathBuf>,
}

fn parse_metadata(output: &str) -> BTreeMap<u32, ProcessMetadata> {
    let mut metadata: BTreeMap<u32, ProcessMetadata> = BTreeMap::new();
    let mut pid = None;
    let mut descriptor = "";
    for field in fields(output) {
        if let Some(value) = field.strip_prefix('p') {
            pid = value.parse().ok();
            descriptor = "";
        } else if let Some(value) = field.strip_prefix('f') {
            descriptor = value;
        } else if let Some(path) = field.strip_prefix('n')
            && let Some(pid) = pid
        {
            let info = metadata.entry(pid).or_default();
            if descriptor == "cwd" {
                info.directory = Some(PathBuf::from(path));
            } else if descriptor == "txt" && info.executable.is_none() {
                // macOS lsof emits the main executable before other mapped files.
                info.executable = Some(PathBuf::from(path));
            }
        }
    }
    metadata
}

fn classify(listener: &Listener, home: &Path) -> Group {
    let paths = [
        listener.directory.as_deref(),
        listener.executable.as_deref(),
    ];
    let local_support = home.join("Library/Application Support/Local");
    if paths
        .into_iter()
        .flatten()
        .any(|path| path.starts_with(&local_support) || path.starts_with("/Applications/Local.app"))
    {
        return Group {
            category: Category::Apps,
            key: "local".into(),
            label: "Local".into(),
        };
    }
    if let Some(directory) = &listener.directory {
        for folder in [
            "Personal",
            "Freelance",
            "Satumaa",
            "Documents/ChatGPT",
            "ChatGPT",
        ] {
            let root = home.join(folder);
            if directory.starts_with(&root) {
                let project_root = find_project_root(directory, &root);
                return Group {
                    category: Category::Projects,
                    key: project_root.display().to_string(),
                    label: project_name(&project_root).unwrap_or_else(|| {
                        project_root
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned()
                    }),
                };
            }
        }
    }
    if let Some(executable) = &listener.executable {
        // Nested helper bundles belong to the outer application.
        if let Some(app) = executable
            .ancestors()
            .filter(|path| path.extension().is_some_and(|ext| ext == "app"))
            .last()
        {
            return Group {
                category: Category::Apps,
                key: app.display().to_string(),
                label: app
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            };
        }
        if executable == &home.join(".browserstack/BrowserStackLocalApp") {
            return Group {
                category: Category::Apps,
                key: "browserstack".into(),
                label: "BrowserStack".into(),
            };
        }
        // Simulator runtimes contain their own System and usr trees.
        let simulator_system = executable.starts_with("/Library/Developer/CoreSimulator")
            && executable.ancestors().any(|ancestor| {
                ancestor
                    .file_name()
                    .is_some_and(|name| name == "RuntimeRoot")
                    && executable.strip_prefix(ancestor).is_ok_and(|relative| {
                        relative.starts_with("System") || relative.starts_with("usr")
                    })
            });
        if executable.starts_with("/usr") || executable.starts_with("/System") || simulator_system {
            return Group {
                category: Category::Apps,
                key: executable.display().to_string(),
                label: listener.process.clone(),
            };
        }
    }
    Group::default()
}

fn find_project_root(directory: &Path, scope: &Path) -> PathBuf {
    for ancestor in directory
        .ancestors()
        .take_while(|path| path.starts_with(scope))
    {
        if ancestor.join("package.json").is_file() || ancestor.join(".git").exists() {
            return ancestor.to_path_buf();
        }
    }
    // With no manifest, use the first folder inside the user's project scope.
    directory
        .strip_prefix(scope)
        .ok()
        .and_then(|path| path.components().next())
        .map(|part| scope.join(part.as_os_str()))
        .unwrap_or_else(|| directory.to_path_buf())
}

fn project_name(directory: &Path) -> Option<String> {
    for ancestor in directory.ancestors() {
        if let Ok(contents) = std::fs::read_to_string(ancestor.join("package.json"))
            && let Ok(package) = serde_json::from_str::<serde_json::Value>(&contents)
            && let Some(name) = package.get("name").and_then(|value| value.as_str())
            && !name.trim().is_empty()
        {
            return Some(name.to_owned());
        }
        // Do not inherit a package name outside the nearest repository.
        if ancestor.join(".git").exists() {
            return ancestor
                .file_name()
                .map(|name| name.to_string_lossy().into_owned());
        }
    }
    directory
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::{Category, classify, parse_listeners, parse_metadata};

    #[test]
    fn groups_dual_stack_sockets_but_preserves_distinct_processes() {
        let rows = parse_listeners(
            "p12\0cnode\0f4\0n*:5173\0\nf5\0n[::1]:5173\0\nf6\0n*:5173\0\np13\0cnode\0f7\0n127.0.0.1:5173\0\n",
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].pid, 12);
        assert_eq!(rows[0].addresses, ["*:5173", "[::1]:5173"]);
        assert_eq!(rows[1].pid, 13);
    }

    #[test]
    fn preserves_working_directory_spaces_and_newlines() {
        let dirs = parse_metadata("p12\0fcwd\0n/Users/me/My Project\nnotes\0\n");
        assert_eq!(
            dirs[&12].directory.as_ref().unwrap(),
            &std::path::PathBuf::from("/Users/me/My Project\nnotes")
        );
        assert!(parse_listeners("pbad\0cnone\0ninvalid\0").is_empty());
    }

    #[test]
    fn classifies_by_path_without_guessing_from_process_name() {
        let home = std::path::Path::new("/Users/me");
        let mut row = parse_listeners("p12\0cnginx\0n*:80\0").remove(0);
        assert_eq!(classify(&row, home).category, Category::Unknown);
        row.executable =
            Some(home.join("Library/Application Support/Local/lightning-services/nginx/bin/nginx"));
        assert_eq!(classify(&row, home).label, "Local");
        assert_eq!(classify(&row, home).category, Category::Apps);
        row.executable = None;
        row.directory = Some(home.join("Personal/site/src"));
        assert_eq!(classify(&row, home).category, Category::Projects);
        row.directory = Some(home.join("Personal-other/site"));
        assert_eq!(classify(&row, home).category, Category::Unknown);
        row.directory = None;
        row.executable = Some("/Applications/Logi Options+.app/Contents/MacOS/agent".into());
        assert_eq!(classify(&row, home).label, "Logi Options+");
        row.executable = Some("/Applications/Discord.app/Contents/Frameworks/Discord Helper.app/Contents/MacOS/Helper".into());
        assert_eq!(classify(&row, home).label, "Discord");
        row.executable = None;
        row.directory = Some(home.join("Personal/no-manifest/one"));
        let first = classify(&row, home);
        row.directory = Some(home.join("Personal/no-manifest/two"));
        assert_eq!(first, classify(&row, home));
    }

    #[test]
    fn identifies_browserstack_and_simulator_system_paths() {
        let home = std::path::Path::new("/Users/me");
        let mut row = parse_listeners("p12\0cBrowserStackLocalApp\0n*:4567\0").remove(0);
        row.executable = Some(home.join(".browserstack/BrowserStackLocalApp"));
        assert_eq!(classify(&row, home).label, "BrowserStack");
        assert_eq!(classify(&row, home).category, Category::Apps);
        row.process = "siriactionsd".into();
        row.executable = Some("/Library/Developer/CoreSimulator/Volumes/iOS/runtime.simruntime/Contents/Resources/RuntimeRoot/System/Library/PrivateFrameworks/VoiceShortcuts.framework/Support/siriactionsd".into());
        assert_eq!(classify(&row, home).category, Category::Apps);
        row.executable = Some("/tmp/siriactionsd".into());
        assert_eq!(classify(&row, home).category, Category::Unknown);
    }

    #[test]
    fn metadata_keeps_executable_separate_from_mapped_libraries() {
        let rows = parse_metadata(
            "p12\0fcwd\0n/\0\nftxt\0n/Applications/Local.app/Contents/MacOS/Local\0\nftxt\0n/usr/lib/dyld\0",
        );
        assert_eq!(
            rows[&12].executable.as_deref(),
            Some(std::path::Path::new(
                "/Applications/Local.app/Contents/MacOS/Local"
            ))
        );
    }

    #[test]
    fn stop_rejects_stale_identity_and_can_terminate_our_child() {
        for force in [false, true] {
            let mut child = std::process::Command::new("/bin/sleep")
                .arg("30")
                .spawn()
                .unwrap();
            let pid = child.id();
            let mut row = parse_listeners(&format!("p{pid}\0csleep\0n*:1234\0")).remove(0);
            row.identity = Some("stale".into());
            assert!(super::stop(&[row.clone()], false).is_err());
            assert!(child.try_wait().unwrap().is_none());
            row.identity = super::process_identities(&pid.to_string())
                .unwrap()
                .remove(&pid);
            let stopped = super::stop(&[row], force);
            if stopped.is_err() {
                let _ = child.kill();
            }
            let status = child.wait().unwrap();
            assert!(stopped.is_ok());
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(status.signal(), Some(if force { 9 } else { 15 }));
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn detects_a_real_listening_socket() {
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        let rows = super::scan().unwrap();
        let row = rows
            .iter()
            .find(|row| row.port == port && row.pid == std::process::id());
        assert!(row.is_some(), "Our own listening socket should appear");
        assert!(row.unwrap().directory.is_some());
        assert!(row.unwrap().executable.is_some());
    }
}
