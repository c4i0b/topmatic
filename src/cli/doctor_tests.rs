use super::doctor::{topgrade_version, version_hint};

#[test]
fn parses_real_version_output() {
    assert_eq!(topgrade_version("topgrade 17.11.0"), Some((17, 11, 0)));
    assert_eq!(topgrade_version("topgrade 17.9.0\n"), Some((17, 9, 0)));
    assert_eq!(topgrade_version("topgrade 18.0"), Some((18, 0, 0)));
}

#[test]
fn rejects_unparseable_output() {
    assert_eq!(topgrade_version(""), None);
    assert_eq!(topgrade_version("topgrade"), None);
    assert_eq!(topgrade_version("topgrade seventeen"), None);
}

#[test]
fn hints_only_below_the_recommended_topgrade() {
    assert_eq!(version_hint("topgrade 17.10.1"), Some((17, 10, 1)));
    assert_eq!(version_hint("topgrade 16.9.0"), Some((16, 9, 0)));
    assert_eq!(version_hint("topgrade 17.11.0"), None);
    assert_eq!(version_hint("topgrade 17.12.3"), None);
    assert_eq!(version_hint("topgrade 18.0.0"), None);
    assert_eq!(version_hint("garbage"), None);
}
