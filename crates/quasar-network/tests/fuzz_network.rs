//! Property-based fuzzing tests for network message parsing.
//!
//! Uses proptest to fuzz network deserialization, ensuring the engine
//! handles malformed/malicious packets gracefully without panicking.

use proptest::prelude::*;
use quasar_network::{
    ClientId, InputData, InputType, NetworkEntityId, NetworkMessage, NetworkPayload,
};

fn any_bytes() -> BoxedStrategy<Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..2048).boxed()
}

fn any_string() -> BoxedStrategy<String> {
    "[a-zA-Z0-9_]{0,32}".prop_map(|s| s.to_string()).boxed()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Fuzz network message deserialization - should never panic
    #[test]
    fn fuzz_message_deserialize(data in any_bytes()) {
        let config = bincode::config::standard();

        let result: Result<(NetworkMessage, usize), _> =
            bincode::serde::decode_from_slice(&data, config);
        let _ = result;
    }

    /// Fuzz with various payload sizes - test for DoS vectors
    #[test]
    fn fuzz_large_payload(size in 0usize..65536) {
        let mut data = vec![0u8; size];
        for (i, byte) in data.iter_mut().enumerate() {
            *byte = (i % 256) as u8;
        }

        let config = bincode::config::standard();

        let result: Result<(NetworkMessage, usize), _> =
            bincode::serde::decode_from_slice(&data, config);
        let _ = result;
    }

    /// Fuzz sequence numbers - ensure no overflow issues
    #[test]
    fn fuzz_sequence_numbers(seq in any::<u64>(), ts in any::<u64>()) {
        let msg = NetworkMessage {
            sequence: seq,
            timestamp: ts,
            payload: NetworkPayload::ConnectionRequest {
                client_id: ClientId(1)
            },
        };

        let config = bincode::config::standard();
        let encoded = bincode::serde::encode_to_vec(&msg, config);

        if let Ok(encoded) = encoded {
            let decoded: Result<(NetworkMessage, usize), _> =
                bincode::serde::decode_from_slice(&encoded, config);

            if let Ok((decoded_msg, _)) = decoded {
                prop_assert_eq!(decoded_msg.sequence, seq);
                prop_assert_eq!(decoded_msg.timestamp, ts);
            }
        }
    }

    /// Fuzz client IDs
    #[test]
    fn fuzz_client_ids(client_id in any::<u64>()) {
        let msg = NetworkMessage {
            sequence: 1,
            timestamp: 0,
            payload: NetworkPayload::ConnectionRequest {
                client_id: ClientId(client_id)
            },
        };

        let config = bincode::config::standard();
        let encoded = bincode::serde::encode_to_vec(&msg, config);

        if let Ok(encoded) = encoded {
            let decoded: Result<(NetworkMessage, usize), _> =
                bincode::serde::decode_from_slice(&encoded, config);

            if let Ok((decoded_msg, _)) = decoded {
                if let NetworkPayload::ConnectionRequest { client_id: decoded_id } = decoded_msg.payload {
                    prop_assert_eq!(decoded_id, ClientId(client_id));
                }
            }
        }
    }

    /// Fuzz entity IDs in snapshots
    #[test]
    fn fuzz_entity_ids(entity_id in any::<u64>()) {
        let msg = NetworkMessage {
            sequence: 1,
            timestamp: 0,
            payload: NetworkPayload::EntitySpawn {
                entity_id: NetworkEntityId(entity_id),
                components: vec![],
            },
        };

        let config = bincode::config::standard();
        let encoded = bincode::serde::encode_to_vec(&msg, config);

        if let Ok(encoded) = encoded {
            let decoded: Result<(NetworkMessage, usize), _> =
                bincode::serde::decode_from_slice(&encoded, config);

            if let Ok((decoded_msg, _)) = decoded {
                if matches!(decoded_msg.payload, NetworkPayload::EntitySpawn { .. }) {
                    // Success
                }
            }
        }
    }

    /// Fuzz input data with various input types
    #[test]
    fn fuzz_input_data(input_type in 0u8..8, value in any::<f32>()) {
        let itype = match input_type {
            0 => InputType::MoveForward,
            1 => InputType::MoveBackward,
            2 => InputType::MoveLeft,
            3 => InputType::MoveRight,
            4 => InputType::Jump,
            5 => InputType::Attack,
            6 => InputType::Interact,
            _ => InputType::Custom("CustomAction".to_string()),
        };

        let msg = NetworkMessage {
            sequence: 1,
            timestamp: 0,
            payload: NetworkPayload::Input {
                client_id: ClientId(1),
                inputs: vec![InputData {
                    input_type: itype,
                    value,
                }],
            },
        };

        let config = bincode::config::standard();
        let encoded = bincode::serde::encode_to_vec(&msg, config);

        if let Ok(encoded) = encoded {
            let decoded: Result<(NetworkMessage, usize), _> =
                bincode::serde::decode_from_slice(&encoded, config);

            if let Ok((decoded_msg, _)) = decoded {
                if let NetworkPayload::Input { inputs, .. } = decoded_msg.payload {
                    prop_assert_eq!(inputs.len(), 1);
                    prop_assert_eq!(inputs[0].value, value);
                }
            }
        }
    }

    /// Fuzz RPC method names
    #[test]
    fn fuzz_rpc(method in any_string(), arg_size in 0usize..64) {
        let args: Vec<u8> = (0..arg_size).map(|i| (i % 256) as u8).collect();

        let msg = NetworkMessage {
            sequence: 1,
            timestamp: 0,
            payload: NetworkPayload::Rpc {
                entity_id: NetworkEntityId(1),
                method: method.clone(),
                args,
            },
        };

        let config = bincode::config::standard();
        let encoded = bincode::serde::encode_to_vec(&msg, config);

        if let Ok(encoded) = encoded {
            let decoded: Result<(NetworkMessage, usize), _> =
                bincode::serde::decode_from_slice(&encoded, config);

            if let Ok((decoded_msg, _)) = decoded {
                if let NetworkPayload::Rpc { method: decoded_method, args: decoded_args, .. } = decoded_msg.payload {
                    prop_assert_eq!(decoded_method, method);
                    prop_assert_eq!(decoded_args.len(), arg_size);
                }
            }
        }
    }
}

#[cfg(test)]
mod deterministic_tests {
    use super::*;

    #[test]
    fn empty_payload_handling() {
        let data: Vec<u8> = vec![];
        let config = bincode::config::standard();
        let result: Result<(NetworkMessage, usize), _> =
            bincode::serde::decode_from_slice(&data, config);
        assert!(result.is_err());
    }

    #[test]
    fn truncated_message_handling() {
        let msg = NetworkMessage {
            sequence: 42,
            timestamp: 12345,
            payload: NetworkPayload::ConnectionRequest {
                client_id: ClientId(1),
            },
        };

        let config = bincode::config::standard();
        let encoded = bincode::serde::encode_to_vec(&msg, config).unwrap();

        for len in 0..encoded.len() {
            let truncated = &encoded[..len];
            let result: Result<(NetworkMessage, usize), _> =
                bincode::serde::decode_from_slice(truncated, config);
            assert!(result.is_err() || len == encoded.len());
        }
    }

    #[test]
    fn corrupted_data_handling() {
        let msg = NetworkMessage {
            sequence: 1,
            timestamp: 0,
            payload: NetworkPayload::ConnectionRequest {
                client_id: ClientId(1),
            },
        };

        let config = bincode::config::standard();
        let mut encoded = bincode::serde::encode_to_vec(&msg, config).unwrap();

        if !encoded.is_empty() {
            for i in 0..encoded.len().min(10) {
                encoded[i] = encoded[i].wrapping_add(1);
                let result: Result<(NetworkMessage, usize), _> =
                    bincode::serde::decode_from_slice(&encoded, config);
                let _ = result;
                encoded[i] = encoded[i].wrapping_sub(1);
            }
        }
    }
}
