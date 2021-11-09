//! Functionality for creating and verifying signatures and hashing.

use crate::{
    error::Error,
    transaction::{Base64, DeepHashItem},
};
use jsonwebkey::JsonWebKey;
use log::debug;
use ring::{
    digest::{Context, SHA256, SHA384},
    rand,
    signature::{self, KeyPair, RsaKeyPair},
};
use std::fs as fsSync;
use std::path::PathBuf;
use tokio::fs;

/// Struct on which [`Methods`] for cryptographic functionality is implemented.
pub struct Provider {
    pub keypair: RsaKeyPair,
}

impl Provider {
    /// Reads a [`JsonWebKey`] from a [`PathBuf`] and stores it as a [`signature::RsaKeyPair`] in
    /// the `keypair` property of [`Provider`] for future use in signing and funding transactions.
    pub async fn from_keypair_path(keypair_path: PathBuf) -> Result<Provider, Error> {
        debug!("{:?}", keypair_path);
        let data = fs::read_to_string(keypair_path).await?;

        let jwk_parsed: JsonWebKey = data.parse().unwrap();
        Ok(Self {
            keypair: signature::RsaKeyPair::from_pkcs8(&jwk_parsed.key.as_ref().to_der())?,
        })
    }
    /// Sync version of [`Provider::from_keypair_path`].
    pub fn from_keypair_path_sync(keypair_path: PathBuf) -> Result<Provider, Error> {
        let data = fsSync::read_to_string(keypair_path)?;

        let jwk_parsed: JsonWebKey = data.parse().unwrap();
        Ok(Self {
            keypair: signature::RsaKeyPair::from_pkcs8(&jwk_parsed.key.as_ref().to_der())?,
        })
    }

    /// Returns the full modulus of the stored keypair. Encoded as a Base64Url String,
    /// represents the associated network address. Also used in the calculation of transaction
    /// signatures.
    pub fn keypair_modulus(&self) -> Result<Base64, Error> {
        let modulus = self
            .keypair
            .public_key()
            .modulus()
            .big_endian_without_leading_zero();
        Ok(Base64(modulus.to_vec()))
    }
    /// Calculates the wallet address of the provided keypair according to [addressing](https://docs.arweave.org/developers/server/http-api#addressing)
    /// in documentation.
    ///```
    /// # use arloader::Arweave;
    /// # use ring::{signature, rand};
    /// # use std::{fmt::Display, path::PathBuf};
    /// #
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let arweave = Arweave::from_keypair_path(PathBuf::from("tests/fixtures/arweave-key-7eV1qae4qVNqsNChg3Scdi-DpOLJPCogct4ixoq1WNg.json"), None).await?;
    /// let calc = arweave.crypto.wallet_address()?;
    /// let actual = String::from("7eV1qae4qVNqsNChg3Scdi-DpOLJPCogct4ixoq1WNg");
    /// assert_eq!(&calc.to_string(), &actual);
    /// # Ok(())
    /// # }
    /// ```
    pub fn wallet_address(&self) -> Result<Base64, Error> {
        let mut context = Context::new(&SHA256);
        context.update(&self.keypair_modulus()?.0[..]);
        let wallet_address = Base64(context.finish().as_ref().to_vec());
        Ok(wallet_address)
    }

    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        let rng = rand::SystemRandom::new();
        let mut signature = vec![0; self.keypair.public_modulus_len()];
        self.keypair
            .sign(&signature::RSA_PSS_SHA256, &rng, message, &mut signature)?;
        Ok(signature)
    }

    /// Verifies that a message was signed by the public key of the Provider.key keypair.
    ///```
    /// # use ring::{signature, rand};
    /// # use arloader::crypto::Provider;
    /// # use std::path::PathBuf;
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let crypto = Provider::from_keypair_path(PathBuf::from("tests/fixtures/arweave-key-7eV1qae4qVNqsNChg3Scdi-DpOLJPCogct4ixoq1WNg.json")).await?;
    /// let message = String::from("hello, world");
    /// let rng = rand::SystemRandom::new();
    /// let signature = crypto.sign(&message.as_bytes())?;
    ///
    /// assert_eq!((), crypto.verify(&signature.as_ref(), &message.as_bytes())?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn verify(&self, signature: &[u8], message: &[u8]) -> Result<(), Error> {
        let public_key = signature::UnparsedPublicKey::new(
            &signature::RSA_PSS_2048_8192_SHA256,
            self.keypair.public_key().as_ref(),
        );
        public_key.verify(message, signature)?;
        Ok(())
    }

    pub fn hash_sha256(&self, message: &[u8]) -> Result<[u8; 32], Error> {
        let mut context = Context::new(&SHA256);
        context.update(message);
        let mut result: [u8; 32] = [0; 32];
        result.copy_from_slice(context.finish().as_ref());
        Ok(result)
    }

    fn hash_sha384(&self, message: &[u8]) -> Result<[u8; 48], Error> {
        let mut context = Context::new(&SHA384);
        context.update(message);
        let mut result: [u8; 48] = [0; 48];
        result.copy_from_slice(context.finish().as_ref());
        Ok(result)
    }

    /// Returns a SHA256 hash of the the concatenated SHA256 hashes of a vector messages.
    pub fn hash_all_sha256(&self, messages: Vec<&[u8]>) -> Result<[u8; 32], Error> {
        let hash: Vec<u8> = messages
            .into_iter()
            .map(|m| self.hash_sha256(m).unwrap())
            .into_iter()
            .flatten()
            .collect();
        let hash = self.hash_sha256(&hash)?;
        Ok(hash)
    }

    /// Returns a SHA384 hash of the the concatenated SHA384 hashes of a vector messages.
    fn hash_all_sha384(&self, messages: Vec<&[u8]>) -> Result<[u8; 48], Error> {
        let hash: Vec<u8> = messages
            .into_iter()
            .map(|m| self.hash_sha384(m).unwrap())
            .into_iter()
            .flatten()
            .collect();
        let hash = self.hash_sha384(&hash)?;
        Ok(hash)
    }

    /// Concatenates two `[u8; 48]` arrays, returning a `[u8; 96]` array.
    fn concat_u8_48(&self, left: [u8; 48], right: [u8; 48]) -> Result<[u8; 96], Error> {
        let mut iter = left.into_iter().chain(right);
        let result = [(); 96].map(|_| iter.next().unwrap());
        Ok(result)
    }

    /// Calculates data root of transaction in accordance with implementation in [arweave-js](https://github.com/ArweaveTeam/arweave-js/blob/master/src/common/lib/deepHash.ts).
    /// [`DeepHashItem`] is a recursive Enum that allows the function to be applied to
    /// nested [`Vec<u8>`] of arbitrary depth.
    pub fn deep_hash(&self, deep_hash_item: DeepHashItem) -> Result<[u8; 48], Error> {
        let hash = match deep_hash_item {
            DeepHashItem::Blob(blob) => {
                let blob_tag = format!("blob{}", blob.len());
                self.hash_all_sha384(vec![blob_tag.as_bytes(), &blob])?
            }
            DeepHashItem::List(list) => {
                let list_tag = format!("list{}", list.len());
                let mut hash = self.hash_sha384(list_tag.as_bytes())?;

                for child in list.into_iter() {
                    let child_hash = self.deep_hash(child)?;
                    hash = self.hash_sha384(&self.concat_u8_48(hash, child_hash)?)?;
                }
                hash
            }
        };
        Ok(hash)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        transaction::{Base64, FromStrs, Tag, ToItems},
        Arweave, Error,
    };
    use std::{path::PathBuf, str::FromStr};

    #[tokio::test]
    async fn test_deep_hash_alt2() -> Result<(), Error> {
        let arweave = Arweave::from_keypair_path(
            PathBuf::from(
                "tests/fixtures/arweave-key-7eV1qae4qVNqsNChg3Scdi-DpOLJPCogct4ixoq1WNg.json",
            ),
            None,
        )
        .await?;

        let file_stems = ["0.png", "1mb.bin"];
        let hashes: [[u8; 48]; 2] = [
            [
                250, 147, 146, 233, 232, 245, 14, 213, 182, 94, 254, 251, 28, 124, 128, 225, 51, 7,
                112, 16, 20, 209, 224, 26, 55, 78, 27, 4, 50, 223, 158, 240, 5, 64, 127, 126, 81,
                156, 153, 245, 207, 219, 8, 108, 158, 120, 212, 214,
            ],
            [
                196, 4, 241, 167, 159, 14, 68, 184, 220, 208, 48, 238, 148, 76, 125, 68, 62, 84,
                192, 99, 165, 188, 36, 73, 249, 200, 16, 52, 193, 249, 190, 60, 85, 148, 252, 195,
                118, 197, 52, 74, 173, 30, 58, 63, 46, 11, 56, 135,
            ],
        ];

        for (file_stem, correct_hash) in file_stems.iter().zip(hashes) {
            let last_tx = Base64::from_str("LCwsLCwsLA")?;
            let other_tags = vec![Tag::from_utf8_strs("key2", "value2")?];
            let transaction = arweave
                .create_transaction_from_file_path(
                    PathBuf::from("tests/fixtures/").join(file_stem),
                    Some(other_tags),
                    Some(last_tx),
                    (0, 0),
                )
                .await?;

            let deep_hash = arweave.crypto.deep_hash(transaction.to_deep_hash_item()?)?;

            assert_eq!(deep_hash, correct_hash);
        }
        Ok(())
    }
}
