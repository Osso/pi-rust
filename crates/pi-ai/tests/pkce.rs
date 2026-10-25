use pi_ai::auth::oauth::pkce::generate_pkce;

#[test]
fn generates_a_base64url_verifier_and_sha256_challenge() {
    let pkce = generate_pkce().unwrap();

    assert_eq!(pkce.verifier.len(), 43);
    assert!(
        pkce.verifier
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
    );
    assert_eq!(pkce.challenge.len(), 43);
    assert!(
        pkce.challenge
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
    );
}
