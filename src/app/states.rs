use sm::sm;

sm! {
  AppSM {
    InitialStates { Main }

    Add {
      Main => NameInput
    }

    Delete {
      Main => ConfirmDelete
    }

    Send {
      Main => SendPop
    }

    Cancel {
      NameInput => Main
      MacInput => Main
      ConfirmAdd => Main
      ConfirmDelete => Main
    }

    Exit {
      Main => Exited
    }

    Next {
      NameInput => MacInput
      MacInput => ConfirmAdd
      ConfirmAdd => Main
      ConfirmDelete => Main
      SendPop => Main
    }
  }
}

pub use AppSM::{Variant::*, *};
