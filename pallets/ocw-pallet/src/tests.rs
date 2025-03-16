use crate::{mock::*, Error, Event, UserInteractions};
use frame_support::{assert_noop, assert_ok};

// // Verify root can successfully set counter value
// #[test]
// fn it_works_for_set_counter_value() {
//     new_test_ext().execute_with(|| {
//         System::set_block_number(1);
//         // Set counter value within max allowed (10)
//         assert_ok!(CustomPallet::set_counter_value(RuntimeOrigin::root(), 5));
//         // Ensure that the correct event is emitted when the value is set
//         System::assert_last_event(Event::CounterValueSet { counter_value: 5 }.into());
//     });
// }
