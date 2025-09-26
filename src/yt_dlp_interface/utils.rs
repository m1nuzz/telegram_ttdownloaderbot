use std::path::PathBuf;

pub fn is_executable_present(path: &PathBuf) -> bool {
    path.exists() && is_executable(path)
}

pub fn is_executable(path: &PathBuf) -> bool {
    #[cfg(windows)]
    {
        path.extension().map_or(false, |ext| ext == "exe")
    }
    #[cfg(not(windows))]{
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).map_or(false, |metadata| {
            let permissions = metadata.permissions();
            permissions.mode() & 0o111 != 0
        })
    }
}