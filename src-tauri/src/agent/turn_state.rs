//! Explicit turn lifecycle, active waiters, and admission control.
use super::{authoring, questions, Approval, PendingApproval, ToolCall};
use std::{
    ops::{Deref, DerefMut},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::{oneshot, watch};

pub(super) struct InteractionReceipt(Option<oneshot::Sender<()>>);

impl Drop for InteractionReceipt {
    fn drop(&mut self) {
        if let Some(done) = self.0.take() {
            let _ = done.send(());
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TurnPhase {
    Reserved,
    Preparing,
    Sampling,
    ExecutingTools,
    WaitingForApproval,
    WaitingForUser,
    WaitingForAgents,
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
    elapsed: Duration,
    running_since: Option<Instant>,
    mailbox: TurnMailbox,
    interactions: Vec<oneshot::Receiver<()>>,
    interactions_cancel: watch::Sender<bool>,
}

impl ActiveTurn {
    pub(super) fn new(id: String, cancel: watch::Sender<bool>) -> Self {
        Self {
            id,
            cancel,
            phase: TurnPhase::Reserved,
            elapsed: Duration::ZERO,
            running_since: None,
            mailbox: TurnMailbox {
                approval: None,
                question: None,
                authoring: None,
                accepting_auxiliary: true,
                delivery: MailboxDelivery::CurrentTurn,
            },
            interactions: Vec::new(),
            interactions_cancel: watch::channel(false).0,
        }
    }

    pub(super) fn transition(&mut self, phase: TurnPhase) {
        self.transition_at(phase, Instant::now());
    }

    fn transition_at(&mut self, phase: TurnPhase, now: Instant) {
        let running = matches!(
            phase,
            TurnPhase::Preparing | TurnPhase::Sampling | TurnPhase::ExecutingTools
        );
        self.set_running_at(running, now);
        self.phase = phase;
        if matches!(phase, TurnPhase::Draining | TurnPhase::Cancelling) {
            self.mailbox.accepting_auxiliary = false;
            self.mailbox.delivery = MailboxDelivery::NextTurn;
        }
    }

    fn set_running_at(&mut self, running: bool, now: Instant) {
        match (self.running_since, running) {
            (Some(start), false) => {
                self.elapsed += now.saturating_duration_since(start);
                self.running_since = None;
            }
            (None, true) => self.running_since = Some(now),
            _ => {}
        }
    }

    pub(super) fn set_delegated_running(&mut self, running: bool) -> bool {
        if self.phase != TurnPhase::WaitingForAgents || self.running_since.is_some() == running {
            return false;
        }
        self.set_running_at(running, Instant::now());
        true
    }

    pub(super) fn doing_work(&self) -> bool {
        self.phase != TurnPhase::WaitingForAgents && self.running_since.is_some()
    }

    pub(super) fn with_elapsed(mut self, duration_ms: u64) -> Self {
        self.elapsed = Duration::from_millis(duration_ms);
        self
    }

    pub(super) fn timing(&self) -> (u64, Option<u64>) {
        (
            self.elapsed_at(Instant::now()),
            self.running_since.map(|_| super::now()),
        )
    }

    fn elapsed_at(&self, now: Instant) -> u64 {
        (self.elapsed
            + self
                .running_since
                .map_or(Duration::ZERO, |start| now.saturating_duration_since(start)))
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
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

    pub(super) fn accepts_interaction(&self) -> bool {
        !*self.cancel.borrow() && !matches!(self.phase, TurnPhase::Draining | TurnPhase::Cancelling)
    }

    pub(super) fn claim_interaction(&mut self) -> InteractionReceipt {
        self.interactions.retain_mut(|receipt| {
            matches!(receipt.try_recv(), Err(oneshot::error::TryRecvError::Empty))
        });
        let (done, receipt) = oneshot::channel();
        self.interactions.push(receipt);
        InteractionReceipt(Some(done))
    }

    pub(super) fn interaction_cleanup_signal(&self) -> watch::Receiver<bool> {
        self.interactions_cancel.subscribe()
    }

    pub(super) fn drain_interactions(&mut self, cancel: bool) -> Vec<oneshot::Receiver<()>> {
        self.transition(TurnPhase::Draining);
        if cancel {
            // Technical cleanup must not masquerade as an explicit user stop.
            self.interactions_cancel.send_replace(true);
        }
        std::mem::take(&mut self.interactions)
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
    fn technical_interaction_cleanup_preserves_explicit_cancellation_channel() {
        let (cancel, user_signal) = watch::channel(false);
        let mut active = ActiveTurn::new("turn".into(), cancel);
        let effect = active.claim_interaction();
        let cleanup = active.interaction_cleanup_signal();
        let mut receipts = active.drain_interactions(true);
        assert!(*cleanup.borrow());
        assert!(!*user_signal.borrow());
        assert!(!active.accepts_interaction());
        assert!(receipts[0].try_recv().is_err());
        // A later real user stop remains observable by workflow recovery.
        active.cancel();
        assert!(*user_signal.borrow());
        drop(effect);
        assert!(receipts[0].try_recv().is_ok());
    }

    #[test]
    fn execution_clock_excludes_queue_human_wait_and_time_between_retries() {
        let (cancel, _) = watch::channel(false);
        let mut active = ActiveTurn::new("turn".into(), cancel).with_elapsed(2_000);
        let start = Instant::now();
        let hour = Duration::from_secs(3_600);
        assert_eq!(active.elapsed_at(start + 9 * hour), 2_000);
        active.transition_at(TurnPhase::Preparing, start + 9 * hour);
        active.transition_at(
            TurnPhase::WaitingForApproval,
            start + 9 * hour + Duration::from_secs(5),
        );
        active.transition_at(TurnPhase::WaitingForUser, start + 10 * hour);
        assert_eq!(active.elapsed_at(start + 18 * hour), 7_000);
        active.transition_at(TurnPhase::ExecutingTools, start + 18 * hour);
        active.transition_at(
            TurnPhase::Draining,
            start + 18 * hour + Duration::from_secs(3),
        );
        assert_eq!(active.elapsed_at(start + 20 * hour), 10_000);
    }

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
