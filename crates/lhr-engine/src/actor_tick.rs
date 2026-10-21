//! Bounded generic actor tick dispatch from the original PC Game.
//!
//! Original Game SHA-256:
//! `08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27`;
//! entry RVA `0x64f0`, contract `PC-ACTOR-TICK-ORACLE-STEP3-20261021`.
//! An unchanged 160-byte body ran in an isolated QEMU i386 harness at its
//! preferred address; 26 controlled synthetic cases passed. This reconstructs
//! only this wrapper's dispatch order. It does not implement the called methods,
//! actor ownership, deletion, or original Game phase integration.

/// Nonowning identity for a model resolved by the actor backend.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ModelId(pub u32);

/// Nonowning identity for the actor's current virtual method table.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ActorVTableId(pub u32);

/// Abstract virtual slots called by the recovered actor wrapper.
///
/// These names identify byte offsets only; they do not assign behavior to the
/// implementations found at those slots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActorVirtualSlot {
    Offset34,
    Offset3c,
    Offset38,
    Offset74,
}

impl ActorVirtualSlot {
    /// Return the original vtable byte offset for this slot.
    pub const fn byte_offset(self) -> u32 {
        match self {
            Self::Offset34 => 0x34,
            Self::Offset3c => 0x3c,
            Self::Offset38 => 0x38,
            Self::Offset74 => 0x74,
        }
    }
}

/// Typed view of the state bytes read or written by the original wrapper.
///
/// `Default` is deliberately not provided: actor construction and initial
/// vtable, model, mask, selector, visibility, and guard values are outside this
/// function contract.
#[derive(Debug, Eq, PartialEq)]
pub struct ActorTickState {
    pub vtable: ActorVTableId,
    pub model: Option<ModelId>,
    /// Raw actor byte at `+0x04`; the wrapper tests `mask & 5 == 0`.
    pub mask: u8,
    /// Raw selector byte at `+0x20`; zero selects slot `+0x34`.
    pub selector: u8,
    /// Raw event gate byte at `+0x5d`.
    pub gate_d: u8,
    /// Raw event gate byte at `+0x5e`.
    pub gate_e: u8,
    /// Visibility byte at `+0x5f`; the wrapper writes only zero or one.
    pub visible: u8,
}

/// Safe adapter errors for memory states outside the native oracle's valid domain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActorTickError {
    /// The native body dereferences this pointer without a null guard.
    MissingModelPointer,
    /// The nonowning model identity cannot be resolved by this backend.
    UnknownModel(ModelId),
    /// A type-1 or type-9 model did not expose its equality token.
    MissingModelEqualityToken(ModelId),
    /// A type-1 or type-9 model requires the global context and scene token.
    MissingSceneEqualityToken,
}

/// Resolves model/context fields and supplies implementations for live virtual
/// dispatch. Lookup methods are pure field reads: they must not call actor code
/// or mutate state. In particular, the model equality token and scene token are
/// queried only for model types 1 and 9, matching the native read order.
///
/// `invoke_virtual` receives the vtable identity captured at the call site and
/// the original signed delta for delta-taking slots. `None` means slot `+0x74`
/// is called without a stack argument. Implementations may immediately mutate
/// `actor` or synchronously reenter `actor.tick(self, delta)`; no actor-field
/// borrow is held across the callback.
pub trait ActorBackend {
    /// Read the model's type DWORD at offset `+0x110`.
    fn model_type(&self, model: ModelId) -> Option<u32>;

    /// Read the model's equality DWORD at offset `+0x13c`.
    fn model_equality_token(&self, model: ModelId) -> Option<u32>;

    /// Read the equality DWORD from global context `+0x10` scene `+0x13c`.
    fn scene_equality_token(&self) -> Option<u32>;

    /// Invoke the currently selected implementation of one abstract slot.
    fn invoke_virtual(
        &mut self,
        actor: &mut ActorTickState,
        vtable: ActorVTableId,
        slot: ActorVirtualSlot,
        delta: Option<i32>,
    );
}

impl ActorTickState {
    /// Run the recovered wrapper's visibility update and virtual call sequence.
    ///
    /// The model pointer is mandatory. Types 1 and 9 set visibility by raw
    /// equality between model and scene tokens; every other type sets it to
    /// one without reading the context chain. If the optional call runs, its
    /// callback may mutate the actor and vtable. Slot `+0x38` then uses the
    /// reloaded vtable. The final live gate check happens after that callback;
    /// when both gates are zero, this stores `gate_e = 1` and `gate_d = 0`
    /// before invoking the then-current vtable's slot `+0x74` without a delta.
    pub fn tick<B: ActorBackend + ?Sized>(
        &mut self,
        backend: &mut B,
        delta: i32,
    ) -> Result<(), ActorTickError> {
        let model = self.model.ok_or(ActorTickError::MissingModelPointer)?;
        let model_type = backend
            .model_type(model)
            .ok_or(ActorTickError::UnknownModel(model))?;
        if model_type == 1 || model_type == 9 {
            let model_token = backend
                .model_equality_token(model)
                .ok_or(ActorTickError::MissingModelEqualityToken(model))?;
            let scene_token = backend
                .scene_equality_token()
                .ok_or(ActorTickError::MissingSceneEqualityToken)?;
            self.visible = u8::from(model_token == scene_token);
        } else {
            self.visible = 1;
        }

        if self.mask & 5 == 0 {
            let slot = if self.selector != 0 {
                ActorVirtualSlot::Offset3c
            } else {
                ActorVirtualSlot::Offset34
            };
            let vtable = self.vtable;
            backend.invoke_virtual(self, vtable, slot, Some(delta));
        }

        let vtable = self.vtable;
        backend.invoke_virtual(self, vtable, ActorVirtualSlot::Offset38, Some(delta));

        if self.gate_d != 0 || self.gate_e != 0 {
            return Ok(());
        }
        let vtable = self.vtable;
        self.gate_e = 1;
        self.gate_d = 0;
        backend.invoke_virtual(self, vtable, ActorVirtualSlot::Offset74, None);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const PRIMARY: ActorVTableId = ActorVTableId(1);
    const ALTERNATE: ActorVTableId = ActorVTableId(2);
    const MODEL: ModelId = ModelId(10);

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct ModelInfo {
        kind: u32,
        token: Option<u32>,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct ObservedCall {
        slot: u16,
        depth: u32,
        visible: u8,
        gate_d: u8,
        gate_e: u8,
        delta: i32,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Action {
        None,
        OptionalSwapVTable,
        OptionalSetD,
        OptionalSetE,
        OptionalChangeModel,
        OptionalReentry,
        UpdateClearGates,
        UpdateSetD,
        UpdateSetE,
        UpdateSwapVTable,
        UpdateReentry,
        EventReentry,
        EventClearEReentry,
    }

    struct Backend {
        models: HashMap<ModelId, ModelInfo>,
        scene_token: Option<u32>,
        events: Vec<ObservedCall>,
        action: Action,
        depth: u32,
        acted: bool,
    }

    impl Backend {
        fn new(
            kind: u32,
            model_token: Option<u32>,
            scene_token: Option<u32>,
            action: Action,
        ) -> Self {
            Self {
                models: HashMap::from([(
                    MODEL,
                    ModelInfo {
                        kind,
                        token: model_token,
                    },
                )]),
                scene_token,
                events: Vec::new(),
                action,
                depth: 0,
                acted: false,
            }
        }

        fn entry(vtable: ActorVTableId, slot: ActorVirtualSlot) -> u16 {
            match (vtable, slot) {
                (ALTERNATE, ActorVirtualSlot::Offset38) => 0x8a,
                (ALTERNATE, ActorVirtualSlot::Offset74) => 0xae,
                (_, slot) => slot.byte_offset() as u16,
            }
        }

        fn fire_reentry(&mut self, actor: &mut ActorTickState, delta: i32) {
            self.acted = true;
            self.depth += 1;
            actor
                .tick(self, delta)
                .expect("valid synthetic actor state");
            self.depth -= 1;
        }
    }

    impl ActorBackend for Backend {
        fn model_type(&self, model: ModelId) -> Option<u32> {
            self.models.get(&model).map(|info| info.kind)
        }

        fn model_equality_token(&self, model: ModelId) -> Option<u32> {
            self.models.get(&model).and_then(|info| info.token)
        }

        fn scene_equality_token(&self) -> Option<u32> {
            self.scene_token
        }

        fn invoke_virtual(
            &mut self,
            actor: &mut ActorTickState,
            vtable: ActorVTableId,
            slot: ActorVirtualSlot,
            delta: Option<i32>,
        ) {
            self.events.push(ObservedCall {
                slot: Self::entry(vtable, slot),
                depth: self.depth,
                visible: actor.visible,
                gate_d: actor.gate_d,
                gate_e: actor.gate_e,
                delta: delta.unwrap_or(0),
            });

            if self.acted {
                return;
            }
            match (self.action, slot) {
                (Action::OptionalSwapVTable, ActorVirtualSlot::Offset34)
                | (Action::OptionalSwapVTable, ActorVirtualSlot::Offset3c) => {
                    self.acted = true;
                    actor.vtable = ALTERNATE;
                }
                (Action::OptionalSetD, ActorVirtualSlot::Offset34)
                | (Action::OptionalSetD, ActorVirtualSlot::Offset3c) => {
                    self.acted = true;
                    actor.gate_d = 1;
                }
                (Action::OptionalSetE, ActorVirtualSlot::Offset34)
                | (Action::OptionalSetE, ActorVirtualSlot::Offset3c) => {
                    self.acted = true;
                    actor.gate_e = 1;
                }
                (Action::OptionalChangeModel, ActorVirtualSlot::Offset34)
                | (Action::OptionalChangeModel, ActorVirtualSlot::Offset3c) => {
                    self.acted = true;
                    self.models.insert(
                        ModelId(11),
                        ModelInfo {
                            kind: 1,
                            token: Some(11),
                        },
                    );
                    actor.model = Some(ModelId(11));
                }
                (Action::OptionalReentry, ActorVirtualSlot::Offset34)
                | (Action::OptionalReentry, ActorVirtualSlot::Offset3c) => {
                    self.fire_reentry(actor, 3);
                }
                (Action::UpdateClearGates, ActorVirtualSlot::Offset38) => {
                    self.acted = true;
                    actor.gate_d = 0;
                    actor.gate_e = 0;
                }
                (Action::UpdateSetD, ActorVirtualSlot::Offset38) => {
                    self.acted = true;
                    actor.gate_d = 1;
                }
                (Action::UpdateSetE, ActorVirtualSlot::Offset38) => {
                    self.acted = true;
                    actor.gate_e = 1;
                }
                (Action::UpdateSwapVTable, ActorVirtualSlot::Offset38) => {
                    self.acted = true;
                    actor.vtable = ALTERNATE;
                }
                (Action::UpdateReentry, ActorVirtualSlot::Offset38) => {
                    self.fire_reentry(actor, 5);
                }
                (Action::EventReentry, ActorVirtualSlot::Offset74) => {
                    self.fire_reentry(actor, 9);
                }
                (Action::EventClearEReentry, ActorVirtualSlot::Offset74) => {
                    self.acted = true;
                    actor.gate_e = 0;
                    self.depth += 1;
                    actor.tick(self, 9).expect("valid synthetic actor state");
                    self.depth -= 1;
                }
                _ => {}
            }
        }
    }

    fn make_actor() -> ActorTickState {
        ActorTickState {
            vtable: PRIMARY,
            model: Some(MODEL),
            mask: 0,
            selector: 0,
            gate_d: 0,
            gate_e: 0,
            visible: 77,
        }
    }

    struct ExpectedNativeVector {
        name: &'static str,
        input_delta: i32,
        trace: &'static [ObservedCall],
        visible_after: u8,
        gate_d_after: u8,
        gate_e_after: u8,
    }

    const fn observed(
        slot: u16,
        depth: u32,
        visible: u8,
        gate_d: u8,
        gate_e: u8,
        delta: i32,
    ) -> ObservedCall {
        ObservedCall {
            slot,
            depth,
            visible,
            gate_d,
            gate_e,
            delta,
        }
    }

    const NATIVE_VECTORS: [ExpectedNativeVector; 26] = [
        ExpectedNativeVector {
            name: "visibility_other",
            input_delta: 7,
            trace: &[
                observed(52, 0, 1, 0, 0, 7),
                observed(56, 0, 1, 0, 0, 7),
                observed(116, 0, 1, 0, 1, 0),
            ],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "visibility_type1_equal",
            input_delta: 7,
            trace: &[
                observed(52, 0, 1, 0, 0, 7),
                observed(56, 0, 1, 0, 0, 7),
                observed(116, 0, 1, 0, 1, 0),
            ],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "visibility_type1_unequal",
            input_delta: 7,
            trace: &[
                observed(52, 0, 0, 0, 0, 7),
                observed(56, 0, 0, 0, 0, 7),
                observed(116, 0, 0, 0, 1, 0),
            ],
            visible_after: 0,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "visibility_type9_equal",
            input_delta: 7,
            trace: &[
                observed(52, 0, 1, 0, 0, 7),
                observed(56, 0, 1, 0, 0, 7),
                observed(116, 0, 1, 0, 1, 0),
            ],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "visibility_type9_unequal",
            input_delta: 7,
            trace: &[
                observed(52, 0, 0, 0, 0, 7),
                observed(56, 0, 0, 0, 0, 7),
                observed(116, 0, 0, 0, 1, 0),
            ],
            visible_after: 0,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "mask1_skips_optional",
            input_delta: 7,
            trace: &[observed(56, 0, 1, 0, 0, 7), observed(116, 0, 1, 0, 1, 0)],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "mask4_skips_optional",
            input_delta: 7,
            trace: &[observed(56, 0, 1, 0, 0, 7), observed(116, 0, 1, 0, 1, 0)],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "mask2_keeps_optional",
            input_delta: 7,
            trace: &[
                observed(52, 0, 1, 0, 0, 7),
                observed(56, 0, 1, 0, 0, 7),
                observed(116, 0, 1, 0, 1, 0),
            ],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "select_slot3c",
            input_delta: 7,
            trace: &[
                observed(60, 0, 1, 0, 0, 7),
                observed(56, 0, 1, 0, 0, 7),
                observed(116, 0, 1, 0, 1, 0),
            ],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "negative_delta",
            input_delta: -7,
            trace: &[
                observed(52, 0, 1, 0, 0, -7),
                observed(56, 0, 1, 0, 0, -7),
                observed(116, 0, 1, 0, 1, 0),
            ],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "minimum_signed_delta",
            input_delta: -2147483648,
            trace: &[
                observed(52, 0, 1, 0, 0, -2147483648),
                observed(56, 0, 1, 0, 0, -2147483648),
                observed(116, 0, 1, 0, 1, 0),
            ],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "guard_d_nonzero",
            input_delta: 7,
            trace: &[observed(52, 0, 1, 2, 0, 7), observed(56, 0, 1, 2, 0, 7)],
            visible_after: 1,
            gate_d_after: 2,
            gate_e_after: 0,
        },
        ExpectedNativeVector {
            name: "guard_e_nonzero",
            input_delta: 7,
            trace: &[observed(52, 0, 1, 0, 2, 7), observed(56, 0, 1, 0, 2, 7)],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 2,
        },
        ExpectedNativeVector {
            name: "optional_swaps_vtable",
            input_delta: 7,
            trace: &[
                observed(52, 0, 1, 0, 0, 7),
                observed(138, 0, 1, 0, 0, 7),
                observed(174, 0, 1, 0, 1, 0),
            ],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "optional_sets_d",
            input_delta: 7,
            trace: &[observed(52, 0, 1, 0, 0, 7), observed(56, 0, 1, 1, 0, 7)],
            visible_after: 1,
            gate_d_after: 1,
            gate_e_after: 0,
        },
        ExpectedNativeVector {
            name: "optional_sets_e",
            input_delta: 7,
            trace: &[observed(52, 0, 1, 0, 0, 7), observed(56, 0, 1, 0, 1, 7)],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "optional_changes_model",
            input_delta: 7,
            trace: &[
                observed(52, 0, 1, 0, 0, 7),
                observed(56, 0, 1, 0, 0, 7),
                observed(116, 0, 1, 0, 1, 0),
            ],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "optional_reentry",
            input_delta: 7,
            trace: &[
                observed(52, 0, 1, 0, 0, 7),
                observed(52, 1, 1, 0, 0, 3),
                observed(56, 1, 1, 0, 0, 3),
                observed(116, 1, 1, 0, 1, 0),
                observed(56, 0, 1, 0, 1, 7),
            ],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "update_clears_guards",
            input_delta: 7,
            trace: &[observed(56, 0, 1, 1, 1, 7), observed(116, 0, 1, 0, 1, 0)],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "update_sets_d",
            input_delta: 7,
            trace: &[observed(56, 0, 1, 0, 0, 7)],
            visible_after: 1,
            gate_d_after: 1,
            gate_e_after: 0,
        },
        ExpectedNativeVector {
            name: "update_sets_e",
            input_delta: 7,
            trace: &[observed(56, 0, 1, 0, 0, 7)],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "update_swaps_vtable",
            input_delta: 7,
            trace: &[observed(56, 0, 1, 0, 0, 7), observed(174, 0, 1, 0, 1, 0)],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "update_reentry",
            input_delta: 7,
            trace: &[
                observed(56, 0, 1, 0, 0, 7),
                observed(56, 1, 1, 0, 0, 5),
                observed(116, 1, 1, 0, 1, 0),
            ],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "event_reentry_guarded",
            input_delta: 7,
            trace: &[
                observed(56, 0, 1, 0, 0, 7),
                observed(116, 0, 1, 0, 1, 0),
                observed(56, 1, 1, 0, 1, 9),
            ],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "event_clears_e_reentry",
            input_delta: 7,
            trace: &[
                observed(56, 0, 1, 0, 0, 7),
                observed(116, 0, 1, 0, 1, 0),
                observed(56, 1, 1, 0, 0, 9),
                observed(116, 1, 1, 0, 1, 0),
            ],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 1,
        },
        ExpectedNativeVector {
            name: "other_type_does_not_read_context",
            input_delta: 7,
            trace: &[
                observed(52, 0, 1, 0, 0, 7),
                observed(56, 0, 1, 0, 0, 7),
                observed(116, 0, 1, 0, 1, 0),
            ],
            visible_after: 1,
            gate_d_after: 0,
            gate_e_after: 1,
        },
    ];

    fn native_scenario(name: &str) -> (ActorTickState, Backend) {
        let (kind, model_token, scene_token, mask, selector, gate_d, gate_e, action) = match name {
            "visibility_type1_equal" => (1, Some(10), Some(10), 0, 0, 0, 0, Action::None),
            "visibility_type1_unequal" => (1, Some(11), Some(10), 0, 0, 0, 0, Action::None),
            "visibility_type9_equal" => (9, Some(10), Some(10), 0, 0, 0, 0, Action::None),
            "visibility_type9_unequal" => (9, Some(11), Some(10), 0, 0, 0, 0, Action::None),
            "mask1_skips_optional" => (2, None, None, 1, 0, 0, 0, Action::None),
            "mask4_skips_optional" => (2, None, None, 4, 0, 0, 0, Action::None),
            "mask2_keeps_optional" => (2, None, None, 2, 0, 0, 0, Action::None),
            "select_slot3c" => (2, None, None, 0, 1, 0, 0, Action::None),
            "negative_delta" | "minimum_signed_delta" => (2, None, None, 0, 0, 0, 0, Action::None),
            "guard_d_nonzero" => (2, None, None, 0, 0, 2, 0, Action::None),
            "guard_e_nonzero" => (2, None, None, 0, 0, 0, 2, Action::None),
            "optional_swaps_vtable" => (2, None, None, 0, 0, 0, 0, Action::OptionalSwapVTable),
            "optional_sets_d" => (2, None, None, 0, 0, 0, 0, Action::OptionalSetD),
            "optional_sets_e" => (2, None, None, 0, 0, 0, 0, Action::OptionalSetE),
            "optional_changes_model" => (
                1,
                Some(10),
                Some(10),
                0,
                0,
                0,
                0,
                Action::OptionalChangeModel,
            ),
            "optional_reentry" => (2, None, None, 0, 0, 0, 0, Action::OptionalReentry),
            "update_clears_guards" => (2, None, None, 1, 0, 1, 1, Action::UpdateClearGates),
            "update_sets_d" => (2, None, None, 1, 0, 0, 0, Action::UpdateSetD),
            "update_sets_e" => (2, None, None, 1, 0, 0, 0, Action::UpdateSetE),
            "update_swaps_vtable" => (2, None, None, 1, 0, 0, 0, Action::UpdateSwapVTable),
            "update_reentry" => (2, None, None, 1, 0, 0, 0, Action::UpdateReentry),
            "event_reentry_guarded" => (2, None, None, 1, 0, 0, 0, Action::EventReentry),
            "event_clears_e_reentry" => (2, None, None, 1, 0, 0, 0, Action::EventClearEReentry),
            "other_type_does_not_read_context" => (2, None, None, 0, 0, 0, 0, Action::None),
            "visibility_other" => (2, None, None, 0, 0, 0, 0, Action::None),
            _ => panic!("unrecognized frozen native scenario: {name}"),
        };
        let mut actor = make_actor();
        actor.mask = mask;
        actor.selector = selector;
        actor.gate_d = gate_d;
        actor.gate_e = gate_e;
        (actor, Backend::new(kind, model_token, scene_token, action))
    }

    #[test]
    fn matches_all_twenty_six_frozen_native_actor_vectors() {
        for vector in &NATIVE_VECTORS {
            let (mut actor, mut backend) = native_scenario(vector.name);
            actor
                .tick(&mut backend, vector.input_delta)
                .unwrap_or_else(|error| panic!("{}: {error:?}", vector.name));
            assert_eq!(
                backend.events, vector.trace,
                "{} callback trace",
                vector.name
            );
            assert_eq!(
                actor.visible, vector.visible_after,
                "{} visibility",
                vector.name
            );
            assert_eq!(actor.gate_d, vector.gate_d_after, "{} gate D", vector.name);
            assert_eq!(actor.gate_e, vector.gate_e_after, "{} gate E", vector.name);
        }
    }

    #[test]
    fn dispatches_visibility_optional_update_and_event_in_order() {
        let mut actor = make_actor();
        let mut backend = Backend::new(2, None, None, Action::None);
        actor.tick(&mut backend, 7).unwrap();
        assert_eq!(
            backend.events,
            [
                ObservedCall {
                    slot: 0x34,
                    depth: 0,
                    visible: 1,
                    gate_d: 0,
                    gate_e: 0,
                    delta: 7
                },
                ObservedCall {
                    slot: 0x38,
                    depth: 0,
                    visible: 1,
                    gate_d: 0,
                    gate_e: 0,
                    delta: 7
                },
                ObservedCall {
                    slot: 0x74,
                    depth: 0,
                    visible: 1,
                    gate_d: 0,
                    gate_e: 1,
                    delta: 0
                },
            ]
        );
        assert_eq!((actor.visible, actor.gate_d, actor.gate_e), (1, 0, 1));
    }

    #[test]
    fn visibility_types_compare_raw_tokens_and_other_types_skip_context() {
        for (kind, model_token, scene_token, visible) in [
            (1, Some(0x1122_3344), Some(0x1122_3344), 1),
            (1, Some(0x1122_3344), Some(0x1122_3345), 0),
            (9, Some(0xaabb_ccdd), Some(0xaabb_ccdd), 1),
            (9, Some(0xaabb_ccdd), Some(0xaabb_ccde), 0),
            (4, None, None, 1),
        ] {
            let mut actor = make_actor();
            let mut backend = Backend::new(kind, model_token, scene_token, Action::None);
            actor.tick(&mut backend, 7).unwrap();
            assert_eq!(actor.visible, visible, "model type {kind}");
        }
    }

    #[test]
    fn mask_and_selector_choose_optional_call_without_normalizing_bytes() {
        for (mask, selector, expected_optional) in [
            (1, 0, None),
            (4, 0, None),
            (2, 0, Some(ActorVirtualSlot::Offset34)),
            (0, 1, Some(ActorVirtualSlot::Offset3c)),
        ] {
            let mut actor = make_actor();
            actor.mask = mask;
            actor.selector = selector;
            let mut backend = Backend::new(2, None, None, Action::None);
            actor.tick(&mut backend, -7).unwrap();
            let observed: Vec<u16> = backend.events.iter().map(|event| event.slot).collect();
            let expected = match expected_optional {
                Some(slot) => vec![slot.byte_offset() as u16, 0x38, 0x74],
                None => vec![0x38, 0x74],
            };
            assert_eq!(observed, expected, "mask {mask}, selector {selector}");
            assert_eq!(backend.events[0].delta, -7);
        }
    }

    #[test]
    fn gate_bytes_are_checked_live_after_callbacks() {
        for (gate_d, gate_e, action, expected_event) in [
            (2, 0, Action::None, false),
            (0, 2, Action::None, false),
            (1, 1, Action::UpdateClearGates, true),
            (0, 0, Action::UpdateSetD, false),
            (0, 0, Action::UpdateSetE, false),
        ] {
            let mut actor = make_actor();
            actor.gate_d = gate_d;
            actor.gate_e = gate_e;
            let mut backend = Backend::new(2, None, None, action);
            actor.tick(&mut backend, 7).unwrap();
            assert_eq!(backend.events.last().unwrap().slot == 0x74, expected_event);
        }
    }

    #[test]
    fn callback_mutations_reload_vtable_model_and_allow_reentry() {
        let mut actor = make_actor();
        let mut backend = Backend::new(2, None, None, Action::OptionalSwapVTable);
        actor.tick(&mut backend, 7).unwrap();
        assert_eq!(
            backend
                .events
                .iter()
                .map(|event| event.slot)
                .collect::<Vec<_>>(),
            [0x34, 0x8a, 0xae]
        );

        let mut actor = make_actor();
        let mut backend = Backend::new(1, Some(10), Some(10), Action::OptionalChangeModel);
        actor.tick(&mut backend, 7).unwrap();
        assert_eq!(
            actor.visible, 1,
            "visibility is not recomputed after callback"
        );
        assert_eq!(actor.model, Some(ModelId(11)));
        assert_eq!(
            backend.models.get(&ModelId(11)),
            Some(&ModelInfo {
                kind: 1,
                token: Some(11),
            }),
            "replacement model token is unequal to the scene token"
        );

        let mut actor = make_actor();
        let mut backend = Backend::new(2, None, None, Action::OptionalReentry);
        actor.tick(&mut backend, 7).unwrap();
        assert_eq!(
            backend
                .events
                .iter()
                .map(|event| (event.slot, event.depth, event.delta))
                .collect::<Vec<_>>(),
            [
                (0x34, 0, 7),
                (0x34, 1, 3),
                (0x38, 1, 3),
                (0x74, 1, 0),
                (0x38, 0, 7)
            ]
        );
    }

    #[test]
    fn update_and_event_callbacks_can_reenter_and_change_gates() {
        for (action, expected) in [
            (
                Action::UpdateReentry,
                vec![
                    (0x34, 0, 7),
                    (0x38, 0, 7),
                    (0x34, 1, 5),
                    (0x38, 1, 5),
                    (0x74, 1, 0),
                ],
            ),
            (
                Action::EventReentry,
                vec![
                    (0x34, 0, 7),
                    (0x38, 0, 7),
                    (0x74, 0, 0),
                    (0x34, 1, 9),
                    (0x38, 1, 9),
                ],
            ),
            (
                Action::EventClearEReentry,
                vec![
                    (0x34, 0, 7),
                    (0x38, 0, 7),
                    (0x74, 0, 0),
                    (0x34, 1, 9),
                    (0x38, 1, 9),
                    (0x74, 1, 0),
                ],
            ),
        ] {
            let mut actor = make_actor();
            let mut backend = Backend::new(2, None, None, action);
            actor.tick(&mut backend, 7).unwrap();
            assert_eq!(
                backend
                    .events
                    .iter()
                    .map(|event| (event.slot, event.depth, event.delta))
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }

    #[test]
    fn invalid_memory_scope_is_reported_instead_of_fabricated() {
        let mut actor = make_actor();
        actor.model = None;
        let mut backend = Backend::new(2, None, None, Action::None);
        assert_eq!(
            actor.tick(&mut backend, 1),
            Err(ActorTickError::MissingModelPointer)
        );
        assert!(backend.events.is_empty());

        let mut actor = make_actor();
        let mut backend = Backend::new(1, Some(1), None, Action::None);
        assert_eq!(
            actor.tick(&mut backend, 1),
            Err(ActorTickError::MissingSceneEqualityToken)
        );
        assert!(backend.events.is_empty());

        let mut actor = make_actor();
        let mut backend = Backend::new(2, None, None, Action::None);
        backend.models.clear();
        assert_eq!(
            actor.tick(&mut backend, 1),
            Err(ActorTickError::UnknownModel(MODEL))
        );
    }
}
