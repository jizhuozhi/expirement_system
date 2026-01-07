use xxhash_rust::xxh3::xxh3_64;
use crate::layer::BUCKET_SIZE;

/// Hash a key with salt to a bucket index
/// Salt ensures different layers produce different distributions for the same key
pub fn hash_to_bucket_with_salt(key: &str, salt: &str) -> u32 {
    // Concatenate key and salt, then hash
    let combined = format!("{}{}", key, salt);
    let hash = xxh3_64(combined.as_bytes());
    (hash % BUCKET_SIZE as u64) as u32
}

/// Hash a key to a bucket index (deprecated, use hash_to_bucket_with_salt)
#[deprecated(note = "Use hash_to_bucket_with_salt to avoid biased distribution")]
pub fn hash_to_bucket(key: &str) -> u32 {
    let hash = xxh3_64(key.as_bytes());
    (hash % BUCKET_SIZE as u64) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    
    #[test]
    fn test_hash_to_bucket_with_salt() {
        let bucket = hash_to_bucket_with_salt("user_123", "layer1_v1");
        assert!(bucket < BUCKET_SIZE);
    }
    
    #[test]
    fn test_hash_determinism() {
        let key = "user_456";
        let salt = "experiment_v2";
        let bucket1 = hash_to_bucket_with_salt(key, salt);
        let bucket2 = hash_to_bucket_with_salt(key, salt);
        assert_eq!(bucket1, bucket2);
    }
    
    #[test]
    fn test_different_salts_produce_different_buckets() {
        let key = "user_789";
        let bucket1 = hash_to_bucket_with_salt(key, "layer1_v1");
        let bucket2 = hash_to_bucket_with_salt(key, "layer2_v1");
        
        // With high probability, different salts should produce different buckets
        // This test may occasionally fail due to hash collision, but probability is very low
        assert_ne!(bucket1, bucket2);
    }
    
    #[test]
    fn test_salt_ensures_different_distribution() {
        let salts = vec!["layer1_v1", "layer2_v1", "layer3_v1"];
        let mut distributions: Vec<HashSet<u32>> = vec![HashSet::new(); salts.len()];
        
        // Test first 100 users
        for i in 0..100 {
            let key = format!("user_{}", i);
            
            for (idx, salt) in salts.iter().enumerate() {
                let bucket = hash_to_bucket_with_salt(&key, salt);
                distributions[idx].insert(bucket);
            }
        }
        
        // Each layer should have reasonable unique buckets
        for dist in &distributions {
            // With 100 users and 10000 buckets, we expect most buckets to be unique
            assert!(dist.len() >= 95, "Expected at least 95 unique buckets, got {}", dist.len());
        }
    }
    
    #[test]
    fn test_hash_distribution() {
        let mut buckets = vec![0; BUCKET_SIZE as usize];
        let salt = "test_layer_v1";
        
        for i in 0..100000 {
            let key = format!("user_{}", i);
            let bucket = hash_to_bucket_with_salt(&key, salt);
            buckets[bucket as usize] += 1;
        }
        
        // Check distribution is reasonable (within 2x of expected)
        let expected = 100000 / BUCKET_SIZE;
        let mut outside_range = 0;
        
        for count in buckets.iter() {
            if *count < expected / 2 || *count > expected * 2 {
                outside_range += 1;
            }
        }
        
        // Allow up to 5% of buckets to be outside expected range
        assert!(outside_range < BUCKET_SIZE / 20);
    }
}
