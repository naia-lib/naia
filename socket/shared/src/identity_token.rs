use crate::Random;

pub type IdentityToken = String;

pub fn generate_identity_token() -> IdentityToken {
    // generate random string of 32 characters
    let mut token = String::new();

    for _ in 0..32 {
        let random_char = std::char::from_u32(Random::gen_range_u32(97, 122)).unwrap();
        token.push(random_char);
    }

    token
}
