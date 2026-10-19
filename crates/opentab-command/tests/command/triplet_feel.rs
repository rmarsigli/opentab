use opentab_command::{Command, CommandError, apply};
use opentab_model::{TRIPLET_FEELS, TripletFeel, from_json, to_json};
use serde_json::json;

use crate::checks::{rejected, round_trip};
use crate::support::fixture;

#[test]
fn set_triplet_feel_replaces_it_and_returns_the_old_one() {
    let mut fx = fixture();
    for feel in TRIPLET_FEELS {
        let command = Command::SetTripletFeel {
            master_bar: 1,
            triplet_feel: feel,
        };
        let old = fx.score.master_bars[1].triplet_feel;
        let inverse = round_trip(&mut fx.score, &command);
        assert_eq!(
            inverse,
            Command::SetTripletFeel {
                master_bar: 1,
                triplet_feel: old,
            }
        );
        apply(&mut fx.score, &command).unwrap();
        assert_eq!(fx.score.master_bars[1].triplet_feel, feel);
        assert_eq!(fx.score.master_bars[0].triplet_feel, TripletFeel::None);
    }
}

#[test]
fn set_triplet_feel_refuses_a_missing_master_bar() {
    let mut fx = fixture();
    let command = Command::SetTripletFeel {
        master_bar: 2,
        triplet_feel: TripletFeel::DottedEighth,
    };
    assert_eq!(
        rejected(&mut fx.score, &command),
        CommandError::IndexOutOfRange {
            what: "master bar",
            index: 2,
            len: 2,
        }
    );
}

#[test]
fn set_triplet_feel_has_the_documented_json_shape() {
    let command = Command::SetTripletFeel {
        master_bar: 3,
        triplet_feel: TripletFeel::ScottishSixteenth,
    };
    let expected = json!({
        "op": "set_triplet_feel",
        "master_bar": 3,
        "triplet_feel": "scottish_sixteenth",
    });
    assert_eq!(serde_json::to_value(&command).unwrap(), expected);
    assert_eq!(
        serde_json::from_value::<Command>(expected).unwrap(),
        command
    );
    let unknown = json!({"op": "set_triplet_feel", "master_bar": 0, "triplet_feel": "swing"});
    assert!(serde_json::from_value::<Command>(unknown).is_err());
}

#[test]
fn the_feel_round_trips_through_json_and_defaults_to_none() {
    let mut fx = fixture();
    fx.score.master_bars[1].triplet_feel = TripletFeel::TripletSixteenth;
    let text = to_json(&fx.score).unwrap();
    assert!(text.contains("\"triplet_feel\": \"triplet_sixteenth\""));
    assert_eq!(from_json(&text).unwrap(), fx.score);
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    for bar in value["master_bars"].as_array_mut().unwrap() {
        bar.as_object_mut().unwrap().remove("triplet_feel");
    }
    let older = from_json(&value.to_string()).unwrap();
    assert!(
        older
            .master_bars
            .iter()
            .all(|bar| bar.triplet_feel == TripletFeel::None)
    );
}
