// filepath: /home/dhruv/dev/fastpay/fastpay_core/src/bridge_committee.rs

use crate::base_types::AuthorityName;
use std::collections::{BTreeMap};

#[derive(Eq, PartialEq, Clone, Hash, Debug)]
pub struct Committee {
    pub voting_rights: BTreeMap<AuthorityName, usize>,
    pub total_votes: usize,
}

impl Committee {
    pub fn new(voting_rights: BTreeMap<AuthorityName, usize>) -> Self {
        let total_votes = voting_rights.iter().fold(0, |sum, (_, votes)| sum + *votes);
        Committee {
            voting_rights,
            total_votes,
        }
    }

    pub fn weight(&self, author: &AuthorityName) -> usize {
        *self.voting_rights.get(author).unwrap_or(&0)
    }

    pub fn quorum_threshold(&self) -> usize {
        // If N = 3f + 1 + k (0 <= k < 3)
        // then (2 N + 3) / 3 = 2f + 1 + (2k + 2)/3 = 2f + 1 + k = N - f
        2 * self.total_votes / 3 + 1
    }

    pub fn validity_threshold(&self) -> usize {
        // If N = 3f + 1 + k (0 <= k < 3)
        // then (N + 2) / 3 = f + 1 + k/3 = f + 1
        (self.total_votes + 2) / 3
    }

    /// Find the highest value than is supported by a quorum of authorities.
    pub fn get_strong_majority_lower_bound<V>(&self, mut values: Vec<(AuthorityName, V)>) -> V
    where
        V: Default + std::cmp::Ord,
    {
        values.sort_by(|(_, x), (_, y)| V::cmp(y, x));
        // Browse values by decreasing order, while tracking how many votes they have.
        let mut score = 0;
        for (name, value) in values {
            score += self.weight(&name);
            if score >= self.quorum_threshold() {
                return value;
            }
        }
        V::default()
    }
}


// /// Load committee configuration from JSON file
// pub fn load_committee_from_file(path: &str) -> Result<Committee, FastPayError> {
//     let json_data = fs::read_to_string(path)
//         .map_err(|_| FastPayError::ConfigurationError)?;
    
//     let members: BTreeMap<String, usize> = serde_json::from_str(&json_data)
//         .map_err(|_| FastPayError::ConfigurationError)?;
    
//     // Convert string keys to AuthorityName
//     let mut authority_members = BTreeMap::new();
//     for (key_str, weight) in members {
//         // Parse public key from hex string
//         let bytes = hex::decode(key_str)
//             .map_err(|_| FastPayError::ConfigurationError)?;
        
//         if bytes.len() != 32 {
//             return Err(FastPayError::ConfigurationError);
//         }
        
//         let mut key_bytes = [0u8; 32];
//         key_bytes.copy_from_slice(&bytes);
        
//         let public_key = PublicKey(key_bytes);
//         let authority = AuthorityName(public_key);
        
//         authority_members.insert(authority, weight);
//     }
    
//     Ok(Committee::new(authority_members))
// }

// /// Save committee configuration to JSON file
// pub fn save_committee_to_file(committee: &Committee, path: &str) -> Result<(), FastPayError> {
//     // Convert AuthorityName to string
//     let mut string_members = BTreeMap::new();
//     for (authority, weight) in &committee.members {
//         let key_str = hex::encode(authority.0.0);
//         string_members.insert(key_str, *weight);
//     }
    
//     // Serialize to JSON
//     let json_data = serde_json::to_string_pretty(&string_members)
//         .map_err(|_| FastPayError::ConfigurationError)?;
    
//     // Ensure directory exists
//     if let Some(parent) = Path::new(path).parent() {
//         fs::create_dir_all(parent)
//             .map_err(|_| FastPayError::ConfigurationError)?;
//     }
    
//     // Write to file
//     let mut file = fs::File::create(path)
//         .map_err(|_| FastPayError::ConfigurationError)?;
    
//     file.write_all(json_data.as_bytes())
//         .map_err(|_| FastPayError::ConfigurationError)?;
    
//     Ok(())
// }