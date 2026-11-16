use i2pr_crypto::RouterIdentityBundle;
use i2pr_proto::Date;
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

pub fn make_bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

pub fn signed_reseed_fixture(now_seconds: u64, signer_id: &str, seed: u64) -> (Vec<u8>, Vec<u8>) {
    use sad_rsa::pkcs8::EncodePrivateKey;
    use sad_rsa::rand_core::SeedableRng;
    use sad_rsa::signature::{SignatureEncoding, Signer};

    let mut rng = rand_chacha_10::ChaCha8Rng::seed_from_u64(seed);
    let private_key = sad_rsa::RsaPrivateKey::new(&mut rng, 4096).expect("RSA-4096 test signer");
    let pkcs8 = private_key.to_pkcs8_der().expect("test signer PKCS8");
    let key_der = rustls_pki_types::PrivatePkcs8KeyDer::from(pkcs8.as_bytes());
    let key_pair = rcgen::KeyPair::from_pkcs8_der_and_sign_algo(&key_der, &rcgen::PKCS_RSA_SHA512)
        .expect("rcgen reads RSA test key");
    let mut certificate_params =
        rcgen::CertificateParams::new(Vec::new()).expect("certificate params");
    certificate_params
        .distinguished_name
        .push(rcgen::DnType::CommonName, signer_id);
    let certificate = certificate_params
        .self_signed(&key_pair)
        .expect("self-signed SU3 trust certificate");

    let router_bundle = make_bundle(seed.saturating_add(1));
    let router_info = i2pr_netdb::LocalRouterInfoBuilder::new(&router_bundle)
        .build_default(Date::from_millis(now_seconds.saturating_mul(1000)))
        .expect("signed fixture RouterInfo");
    let hash = i2pr_netdb::encode(router_info.router_hash().as_bytes()).expect("I2P base64 hash");
    let entry_name = format!("routerInfo-{hash}.dat");
    let mut archive = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut archive);
        let mut zip = zip::ZipWriter::new(cursor);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file(entry_name, options)
            .expect("start RouterInfo entry");
        std::io::Write::write_all(
            &mut zip,
            &router_info
                .encoded(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
                .expect("encode RouterInfo"),
        )
        .expect("write RouterInfo");
        zip.finish().expect("finish test archive");
    }

    let signature_length = 512_usize;
    let version = now_seconds.to_string();
    let mut padded_version = [0_u8; 16];
    padded_version[..version.len()].copy_from_slice(version.as_bytes());
    let mut su3 = Vec::new();
    su3.extend_from_slice(b"I2Psu3");
    su3.extend_from_slice(&[0, 0]);
    su3.extend_from_slice(&6_u16.to_be_bytes());
    su3.extend_from_slice(&(signature_length as u16).to_be_bytes());
    su3.extend_from_slice(&[0, 16, 0, signer_id.len() as u8]);
    su3.extend_from_slice(&(archive.len() as u64).to_be_bytes());
    su3.extend_from_slice(&[0, 0, 0, 3]);
    su3.extend_from_slice(&[0; 12]);
    su3.extend_from_slice(&padded_version);
    su3.extend_from_slice(signer_id.as_bytes());
    su3.extend_from_slice(&archive);
    let signing_key = sad_rsa::pkcs1v15::SigningKey::<sad_rsa::sha2::Sha512>::new(private_key);
    let signature = signing_key.sign(&su3);
    assert_eq!(signature.to_bytes().len(), signature_length);
    su3.extend_from_slice(&signature.to_bytes());
    (su3, certificate.der().to_vec())
}
