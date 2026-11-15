//! The `.roster` container: Roster's own encrypted export/import format.
//!
//! One file holds every selected account plus the Donut profile archive(s) that
//! carry their sessions, sealed with a password. The crypto is Donut's
//! (`sync::encryption`: Argon2id key derivation + AES-256-GCM), so this file
//! only defines the container shape and the pack/unpack round-trip — both
//! unit-tested below without touching disk or a real profile.
//!
//! On-disk layout (so a wrong password or a truncated file fails cleanly before
//! anything is created):
//! ```text
//! ROSTER1\n
//! <base64 salt>\n
//! <AES-256-GCM ciphertext of the JSON bundle>
//! ```

use serde::{Deserialize, Serialize};

use crate::account_manager::GoogleAccount;
use crate::sync::encryption::{derive_profile_key, generate_salt};

const MAGIC: &str = "ROSTER1";
const FORMAT_VERSION: u32 = 1;

/// One profile's portable archive bytes (a `.donutprofile` zip from
/// `profile::portable`) travelling inside the bundle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileBlob {
  pub profile_id: String,
  pub name: String,
  /// The raw bytes of the profile's portable archive. Plain `Vec<u8>` (serde
  /// encodes it as a number array) to avoid a dependency; it rides inside the
  /// encrypted payload, and the whole bundle is written once, so the size cost
  /// is not worth a new crate.
  pub archive: Vec<u8>,
}

/// Everything a `.roster` file carries, before encryption.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RosterBundle {
  pub format_version: u32,
  pub accounts: Vec<GoogleAccount>,
  pub profiles: Vec<ProfileBlob>,
}

impl RosterBundle {
  pub fn new(accounts: Vec<GoogleAccount>, profiles: Vec<ProfileBlob>) -> Self {
    Self {
      format_version: FORMAT_VERSION,
      accounts,
      profiles,
    }
  }
}

/// Seal a bundle into `.roster` file bytes with `password`. A fresh random salt
/// is minted per export, so the same bundle and password never produce the same
/// ciphertext twice.
pub fn pack(bundle: &RosterBundle, password: &str) -> Result<Vec<u8>, String> {
  // `generate_salt` and `derive_profile_key` share one base64 alphabet, so the
  // salt string is stored verbatim and never re-encoded through a second one.
  let salt = generate_salt();
  let key = derive_profile_key(password, &salt)?;
  let plaintext = serde_json::to_vec(bundle).map_err(|e| e.to_string())?;
  let ciphertext = crate::sync::encryption::encrypt_bytes(&key, &plaintext)?;

  let mut out = Vec::new();
  out.extend_from_slice(MAGIC.as_bytes());
  out.push(b'\n');
  out.extend_from_slice(salt.as_bytes());
  out.push(b'\n');
  out.extend_from_slice(&ciphertext);
  Ok(out)
}

/// Open `.roster` file bytes with `password`. A wrong password or any tampering
/// fails the AES-GCM tag and returns `WRONG_PASSWORD_OR_CORRUPT`, before the
/// caller touches anything on disk.
pub fn unpack(file_bytes: &[u8], password: &str) -> Result<RosterBundle, String> {
  let mut parts = file_bytes.splitn(3, |b| *b == b'\n');
  let magic = parts.next().ok_or("NOT_A_ROSTER_FILE")?;
  if magic != MAGIC.as_bytes() {
    return Err("NOT_A_ROSTER_FILE".to_string());
  }
  let salt_line = parts.next().ok_or("NOT_A_ROSTER_FILE")?;
  let ciphertext = parts.next().ok_or("NOT_A_ROSTER_FILE")?;

  let salt = String::from_utf8_lossy(salt_line);
  let key = derive_profile_key(password, &salt)?;
  let plaintext = crate::sync::encryption::decrypt_bytes(&key, ciphertext)
    .map_err(|_| "WRONG_PASSWORD_OR_CORRUPT".to_string())?;

  let bundle: RosterBundle =
    serde_json::from_slice(&plaintext).map_err(|_| "WRONG_PASSWORD_OR_CORRUPT".to_string())?;
  if bundle.format_version > FORMAT_VERSION {
    return Err("ROSTER_FILE_TOO_NEW".to_string());
  }
  Ok(bundle)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::account_manager::AccountStatus;

  fn sample_bundle() -> RosterBundle {
    RosterBundle::new(
      vec![GoogleAccount {
        id: "a1".to_string(),
        email: "andi@gmail.com".to_string(),
        display_name: Some("Andi".to_string()),
        avatar_url: None,
        profile_id: "p1".to_string(),
        authuser_index: 0,
        status: AccountStatus::Active,
        last_refresh: Some(100),
        tags: vec!["klien-a".to_string()],
        note: None,
      }],
      vec![ProfileBlob {
        profile_id: "p1".to_string(),
        name: "Slot 1".to_string(),
        archive: vec![1, 2, 3, 4, 5],
      }],
    )
  }

  #[test]
  fn pack_then_unpack_round_trips_the_bundle() {
    let bundle = sample_bundle();
    let file = pack(&bundle, "s3cret").unwrap();
    let back = unpack(&file, "s3cret").unwrap();
    assert_eq!(back, bundle);
  }

  #[test]
  fn a_wrong_password_is_refused_not_misread() {
    let file = pack(&sample_bundle(), "right").unwrap();
    let err = unpack(&file, "wrong").unwrap_err();
    assert_eq!(err, "WRONG_PASSWORD_OR_CORRUPT");
  }

  #[test]
  fn tampering_with_the_ciphertext_is_caught() {
    let mut file = pack(&sample_bundle(), "pw").unwrap();
    let last = file.len() - 1;
    file[last] ^= 0xff;
    assert!(unpack(&file, "pw").is_err());
  }

  #[test]
  fn a_foreign_file_is_rejected_by_magic() {
    assert_eq!(
      unpack(b"NOTROSTER\n...", "pw").unwrap_err(),
      "NOT_A_ROSTER_FILE"
    );
  }

  #[test]
  fn the_same_bundle_and_password_pack_differently_each_time() {
    let bundle = sample_bundle();
    // Fresh salt per export, so two exports are not byte-identical.
    assert_ne!(pack(&bundle, "pw").unwrap(), pack(&bundle, "pw").unwrap());
  }
}
