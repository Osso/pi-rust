use pi_ai::auth::oauth::pkce::generate_pkce;

#[test]
fn generates_a_base64url_verifier_and_sha256_challenge() {
    let pkce = generate_pkce();

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
    assert_eq!(pkce.challenge, pi_ai::auth::oauth::pkce::challenge_for(&pkce.verifier));
}

#[test]
fn challenges_match_known_sha256_vectors() {
    assert_eq!(
        pi_ai::auth::oauth::pkce::challenge_for("verifier"),
        "iMnq5o6zALKXGivsnlom_0F5_WYda32GHkxlV7mq7hQ"
    );
}
