use app_lib::ai::service::parse_pipeline_executables;

#[test]
fn parses_disk_diagnostics_fixture() {
    let command = include_str!("fixtures/disk_diagnostics.sh");

    assert_eq!(
        parse_pipeline_executables(command).unwrap(),
        vec!["echo", "df", "du", "sort", "find", "head", "apt-mark", "wc"],
    );
}

#[test]
fn parses_shell_control_operators_redirects_and_command_substitutions() {
    let command = r#"
        output=$(mktemp)
        sudo systemctl restart nginx && cat "$output" || echo "restart failed"
        rm -f "$output" 2>/dev/null
    "#;

    assert_eq!(
        parse_pipeline_executables(command).unwrap(),
        vec!["mktemp", "sudo", "systemctl", "cat", "echo", "rm"],
    );
}

#[test]
fn permits_administrator_commands_without_an_execution_whitelist() {
    assert_eq!(
        parse_pipeline_executables("find /var/log -type f -exec gzip -9 {} \\;").unwrap(),
        vec!["find", "gzip"],
    );
    assert_eq!(
        parse_pipeline_executables("cat /root/.ssh/authorized_keys").unwrap(),
        vec!["cat"],
    );
}

#[test]
fn rejects_dynamic_or_indirect_execution() {
    for command in [
        "eval \"rm -rf /tmp/example\"",
        "source ./maintenance.sh",
        ". ./maintenance.sh",
        "bash -c 'rm -rf /tmp/example'",
        "env sh -c 'id'",
        "printf '%s\\n' id | xargs -n1 sh -c",
    ] {
        assert!(parse_pipeline_executables(command).is_err(), "{command}");
    }
}

#[test]
fn rejects_invalid_bash_and_dynamic_command_names() {
    assert!(parse_pipeline_executables("echo 'unterminated").is_err());
    assert!(parse_pipeline_executables("$command --all").is_err());
}
