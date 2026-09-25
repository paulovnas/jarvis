//! Explicit turn lifecycle, active waiters, and admission control.
use super::{authoring, questions, Approval, PendingApproval, ToolCall};
use std::{
    ops::{Deref, DerefMut},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::sync::watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TurnPhase {
    Reserved,
    Preparing,
    Sampling,
    ExecutingTools,
    WaitingForApproval,
    WaitingForUser,
    Draining,
    Cancelling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MailboxDelivery {
    CurrentTurn,
    NextTurn,
}

pub(super) struct TurnMailbox {
    approval: Option<Approval>,
    question: Option<questions::Pending>,
    authoring: Option<authoring::Pending>,
    accepting_auxiliary: bool,
    delivery: MailboxDelivery,
}

pub(super) struct ActiveTurn {
    pub id: String,
    pub cancel: watch::Sender<bool>,
    pub phase: TurnPhase,
    mailbox: TurnMailbox,
}

impl ActiveTurn {
    pub(super) fn new(id: String, cancel: watch::Sender<bool>) -> Self {
        Self {
            id,
            cancel,
            phase: TurnPhase::Reserved,
            mailbox: TurnMailbox {
                approval: None,
                question: None,
                authoring: None,
                accepting_auxiliary: true,
                delivery: MailboxDelivery::CurrentTurn,
            },
        }
    }

    pub(super) fn transition(&mut self, phase: TurnPhase) {
        self.phase = phase;
        if matches!(phase, TurnPhase::Draining | TurnPhase::Cancelling) {
            self.mailbox.accepting_auxiliary = false;
            self.mailbox.delivery = MailboxDelivery::NextTurn;
        }
    }

    pub(super) fn is_waiting(&self) -> bool {
        self.mailbox.approval.is_some()
            || self.mailbox.question.is_some()
            || self.mailbox.authoring.is_some()
    }

    pub(super) fn pending_approval(&self) -> Option<&Approval> {
        self.mailbox.approval.as_ref()
    }

    pub(super) fn pending_approval_tool(&self) -> Option<&ToolCall> {
        self.pending_approval()
            .map(|approval| &approval.request.tool)
    }

    pub(super) fn pending_approval_request(&self) -> Option<&PendingApproval> {
        self.pending_approval().map(|approval| &approval.request)
    }

    pub(super) fn wait_for_approval(&mut self, approval: Approval) {
        self.transition(TurnPhase::WaitingForApproval);
        self.mailbox.approval = Some(approval);
    }

    pub(super) fn take_approval(&mut self, tool_id: &str) -> Option<Approval> {
        if !self
            .pending_approval_tool()
            .is_some_and(|tool| tool.id == tool_id)
        {
            return None;
        }
        self.mailbox.approval.take()
    }

    pub(super) fn clear_approval(&mut self) {
        self.mailbox.approval = None;
        self.transition(TurnPhase::ExecutingTools);
    }

    pub(super) fn pending_question(&self) -> Option<&questions::Pending> {
        self.mailbox.question.as_ref()
    }

    pub(super) fn pending_question_mut(&mut self) -> Option<&mut questions::Pending> {
        self.mailbox.question.as_mut()
    }

    pub(super) fn wait_for_question(&mut self, pending: questions::Pending) {
        self.transition(TurnPhase::WaitingForUser);
        self.mailbox.question = Some(pending);
    }

    pub(super) fn take_question(&mut self) -> Option<questions::Pending> {
        let pending = self.mailbox.question.take();
        if pending.is_some() {
            self.transition(TurnPhase::ExecutingTools);
        }
        pending
    }

    pub(super) fn pending_authoring(&self) -> Option<&authoring::Pending> {
        self.mailbox.authoring.as_ref()
    }

    pub(super) fn wait_for_authoring(&mut self, pending: authoring::Pending) {
        self.transition(TurnPhase::WaitingForUser);
        self.mailbox.authoring = Some(pending);
    }

    pub(super) fn take_authoring(&mut self) -> Option<authoring::Pending> {
        let pending = self.mailbox.authoring.take();
        if pending.is_some() {
            self.transition(TurnPhase::ExecutingTools);
        }
        pending
    }

    pub(super) fn accepts_auxiliary(&self) -> bool {
        self.mailbox.accepting_auxiliary && self.mailbox.delivery == MailboxDelivery::CurrentTurn
    }

    pub(super) fn close_auxiliary(&mut self) {
        self.mailbox.accepting_auxiliary = false;
        self.mailbox.delivery = MailboxDelivery::NextTurn;
    }

    pub(super) fn cancel(&mut self) {
        self.transition(TurnPhase::Cancelling);
        let _ = self.cancel.send(true);
    }
}

impl Deref for ActiveTurn {
    type Target = TurnMailbox;

    fn deref(&self) -> &Self::Target {
        &self.mailbox
    }
}

impl DerefMut for ActiveTurn {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.mailbox
    }
}

#[derive(Clone, Default)]
pub(super) struct TurnAdmission {
    shared: Arc<AdmissionState>,
}

#[derive(Default)]
struct AdmissionState {
    draining: AtomicBool,
    active: AtomicUsize,
}

pub(crate) struct TurnLease {
    shared: Arc<AdmissionState>,
}

pub(crate) struct DrainLease {
    shared: Arc<AdmissionState>,
    release_on_drop: bool,
}

impl TurnAdmission {
    pub(super) fn enter(&self) -> Result<TurnLease, &'static str> {
        if self.shared.draining.load(Ordering::Acquire) {
            return Err("O Jarvis está encerrando ou aplicando uma atualização.");
        }
        self.shared.active.fetch_add(1, Ordering::AcqRel);
        if self.shared.draining.load(Ordering::Acquire) {
            self.shared.active.fetch_sub(1, Ordering::AcqRel);
            return Err("O Jarvis está encerrando ou aplicando uma atualização.");
        }
        Ok(TurnLease {
            shared: self.shared.clone(),
        })
    }

    pub(super) fn begin_drain(&self) -> Result<DrainLease, &'static str> {
        if self
            .shared
            .draining
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err("O Jarvis já está preparando o encerramento.");
        }
        if self.shared.active.load(Ordering::Acquire) != 0 {
            self.shared.draining.store(false, Ordering::Release);
            return Err("Aguarde as conversas em execução antes de atualizar.");
        }
        Ok(DrainLease {
            shared: self.shared.clone(),
            release_on_drop: true,
        })
    }

    pub(super) fn active(&self) -> usize {
        self.shared.active.load(Ordering::Acquire)
    }
}

impl Drop for TurnLease {
    fn drop(&mut self) {
        self.shared.active.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Drop for DrainLease {
    fn drop(&mut self) {
        if self.release_on_drop {
            self.shared.draining.store(false, Ordering::Release);
        }
    }
}

impl DrainLease {
    pub(crate) fn keep_closed(mut self) {
        self.release_on_drop = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn drain_rejects_new_turns_and_only_starts_when_idle() {
        let admission = TurnAdmission::default();
        let active = admission.enter().unwrap();
        assert!(admission.begin_drain().is_err());
        drop(active);
        let drain = admission.begin_drain().unwrap();
        assert!(admission.enter().is_err());
        drop(drain);
        assert!(admission.enter().is_ok());
    }

    #[test]
    fn committed_drain_keeps_new_turns_closed_for_process_exit() {
        let admission = TurnAdmission::default();
        admission.begin_drain().unwrap().keep_closed();
        assert!(admission.enter().is_err());
    }

    #[test]
    fn mailbox_closes_auxiliary_delivery_deterministically_during_drain_and_cancel() {
        let (cancel, mut signal) = watch::channel(false);
        let mut active = ActiveTurn::new("turn".into(), cancel);
        assert!(active.accepts_auxiliary());
        active.transition(TurnPhase::Draining);
        assert!(!active.accepts_auxiliary());

        let (cancel, signal_after_cancel) = watch::channel(false);
        let mut cancelled = ActiveTurn::new("cancelled".into(), cancel);
        cancelled.cancel();
        assert!(!cancelled.accepts_auxiliary());
        assert!(*signal_after_cancel.borrow());
        assert!(!*signal.borrow_and_update());
    }

    #[test]
    fn approval_waiter_is_correlated_by_tool_and_returns_to_execution() {
        let (cancel, _) = watch::channel(false);
        let (reply, received) = tokio::sync::oneshot::channel();
        let mut active = ActiveTurn::new("turn".into(), cancel);
        active.wait_for_approval(Approval::new(
            ToolCall {
                id: "tool".into(),
                name: "write".into(),
                args: json!({"path":"a.txt"}),
                status: "pending".into(),
                output: String::new(),
                duration_ms: 0,
            },
            None,
            None,
            None,
            reply,
        ));
        assert_eq!(active.phase, TurnPhase::WaitingForApproval);
        assert!(active.is_waiting());
        assert!(active.take_approval("other").is_none());
        let approval = active.take_approval("tool").unwrap();
        approval.reply.send(true).unwrap();
        assert!(received.blocking_recv().unwrap());
        active.clear_approval();
        assert_eq!(active.phase, TurnPhase::ExecutingTools);
        assert!(!active.is_waiting());
    }
}
