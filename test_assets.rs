fn contains_path_substring_ignore_case(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    if haystack.len() < needle.len() {
        return false;
    }

    let needle_bytes = needle.as_bytes();
    haystack.as_bytes().windows(needle_bytes.len()).any(|window| {
        window.iter().zip(needle_bytes.iter()).all(|(&h, &n)| {
            let h_norm = if h == b'\\' { b'/' } else { h };
            let n_norm = if n == b'\\' { b'/' } else { n };
            h_norm.eq_ignore_ascii_case(&n_norm)
        })
    })
}

fn main() {
    assert!(contains_path_substring_ignore_case("C:\\Program Files\\RocketLeague\\Binaries", "rocketleague/binaries"));
    assert!(contains_path_substring_ignore_case("rocketleague/binaries", "rocketleague/binaries"));
    assert!(contains_path_substring_ignore_case("RocketLeague_EAC.exe", "rocketleague_eac.exe"));
    assert!(!contains_path_substring_ignore_case("steamwebhelper", "rocketleague.exe"));
    println!("Tests passed!");
}
