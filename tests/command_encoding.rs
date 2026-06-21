use lazyifconfig::command::decode_command_output;

#[test]
fn decodes_utf8_command_output() {
    assert_eq!(decode_command_output("한글 route".as_bytes()), "한글 route");
}

#[cfg(target_os = "windows")]
#[test]
fn decodes_windows_korean_command_output() {
    let cp949_bytes = [0xc7, 0xd1, 0xb1, 0xdb, b' ', b'r', b'o', b'u', b't', b'e'];

    assert_eq!(decode_command_output(&cp949_bytes), "한글 route");
}
