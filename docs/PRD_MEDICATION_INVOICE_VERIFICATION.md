# PRD: Medication Invoice Verification for Patient-Specific Approval

## Business Context
Pharmacies and specialty providers submit invoices for dispensed medications to insurers and health plan administrators. Payment integrity teams currently review many of these invoices manually or through fragmented rule systems. This product introduces a natural-language rules engine workflow to verify that each invoiced prescription aligns with a patient-specific approved medication list (formulary + prior authorization decisions) before payment is released.

Key operational context:
- Medication costs are rising, especially in specialty categories.
- Incorrect approvals and post-payment recoveries are expensive and slow.
- Teams need auditable decision logic that compliance, operations, and business stakeholders can read.

## Problem Space / User Research

### Qualitative Signals
- Claims analysts report that policy interpretation varies by reviewer.
- Pharmacy operations teams describe frequent exceptions (generic substitutions, dosage changes, refill timing).
- Compliance teams need transparent rationale for denials and approvals.
- Engineering teams report long cycle times to change hard-coded validation logic.

### Quantitative Signals (Imaginary Baseline for Product Framing)
- 8-12% of medication invoices require manual review.
- 2.5-4.0% of paid invoices are later flagged for discrepancy review.
- Manual review averages 9-15 minutes per invoice.
- Rule-change lead time is typically 2-4 weeks due to cross-team dependencies.
- Post-payment recovery success is below 55% for ineligible medication spend.

## Problem Statement
Medication invoice validation is inconsistent, slow, and difficult to audit. The product will solve this by providing a deterministic, explainable verification flow that checks each invoice against a patient-specific approved medication list and produces a clear approval/deny decision with reasons.

## Who Is This For?
- Payment Integrity Analyst
  - Needs consistent, explainable decisions and reduced manual triage.
- Pharmacy Operations Manager
  - Needs faster adjudication with fewer escalations.
- Compliance/Audit Lead
  - Needs traceable logic and reproducible outcomes for regulatory review.
- Product/Rules Administrator
  - Needs to update policy logic safely and quickly.
- Data/Engineering Team
  - Needs stable interfaces to patient, medication, and authorization services.

## Why This Problem Needs To Be Solved Now
- Specialty medication spend is increasing and magnifies leakage risk.
- Regulatory and payer audit pressure is increasing for explainability.
- Existing rule systems are brittle and costly to modify.
- Delayed adjudication hurts provider relationships and patient experience.

## Success Outcomes and KPIs

Primary outcomes:
- Increase first-pass automated verification rate.
- Reduce ineligible payments caused by medication mismatch.
- Reduce manual review effort per invoice.
- Improve audit readiness with deterministic evidence trails.

KPIs:
- Auto-adjudication rate: target +25% relative improvement in 2 quarters.
- False approval rate (ineligible invoice approved): target <0.5%.
- False denial rate (eligible invoice denied): target <1.0%.
- Manual review time per invoice: target -40%.
- Rule change lead time: target from weeks to <3 business days.
- Decision trace completeness: target 100% decisions with machine-readable rationale.

## Assumptions
- A reliable patient identifier can be resolved from each invoice.
- Approved medication lists are available and up to date at decision time.
- Medication normalization (NDC/brand/generic mapping) can be performed with acceptable accuracy.
- Prior authorization metadata is queryable (effective dates, quantity limits, refill constraints).
- Business stakeholders can define policy exceptions in controlled natural language.
- Deterministic execution is preferred over probabilistic adjudication.

## Requirements

### Jobs To Be Done
1. As a payment integrity analyst, I need each invoice checked against a patient-specific approved medication list so I can trust first-pass decisions.
2. As an operations manager, I need clear deny reasons so exception queues are actionable.
3. As a compliance lead, I need auditable evidence that explains exactly why a decision was made.
4. As a rules administrator, I need to update and test policy logic without deep engineering work.
5. As an engineer, I need clear service boundaries for data retrieval, normalization, and decision output.

### Functional Requirements
- Ingest invoice payload with patient ID, medication details, quantities, dates, and prescriber fields.
- Resolve patient profile and approved medication list for service date.
- Normalize medication identity (brand/generic/NDC equivalence).
- Validate:
  - medication is approved for patient at service date
  - quantity and refill limits are within authorization bounds
  - prescriber and plan constraints are satisfied (where configured)
- Output deterministic status: `approved`, `denied`, or `needs_review`.
- Return structured rationale and rule-level evidence.
- Write an immutable decision log entry.

### Non-Functional Requirements
- Deterministic execution with reproducible outcomes.
- p95 decision latency under agreed SLA (example: <2 seconds for standard checks).
- High observability for failures and data quality issues.
- Role-appropriate visibility for compliance and operations users.

## Out of Scope
- Clinical efficacy determination or treatment appropriateness.
- Real-time prior authorization creation workflows.
- Full pharmacy claims adjudication replacement.
- Benefit design authoring UI in this phase.
- Provider contract pricing negotiation logic.
- International coding standards beyond initial market scope.

## Open Questions

| ID | Question | Owner | Target Date | Status |
|----|----------|-------|-------------|--------|
| OQ-1 | What is the canonical source of truth when formulary and prior auth conflict? | Product + Compliance | TBD | Open |
| OQ-2 | Which medication normalization source is authoritative for brand/generic/NDC mapping? | Data Engineering | TBD | Open |
| OQ-3 | What confidence threshold routes to `needs_review` vs `denied` for ambiguous mappings? | Product + Ops | TBD | Open |
| OQ-4 | What is the minimal rationale schema required for external audits? | Compliance | TBD | Open |
| OQ-5 | Which exception policies differ by payer line of business and need tenant scoping? | Product | TBD | Open |
| OQ-6 | What SLA is required for batch reprocessing during upstream outages? | Operations | TBD | Open |

---

## Devlish Translation Seed (Commented for LLM Generation)

```dvl
# Purpose:
# Verify a medication invoice against a patient-specific approved medication list.
# This is a seed program in plain English intended for translation into working Devlish.
# Comments are intentionally detailed to help an LLM preserve intent and edge-case handling.

# -----------------------------------------------------------------------------
# TERMS AND STRUCTURE
# -----------------------------------------------------------------------------
# Service-like terms (Proper Noun style):
# - PatientService: fetch patient profile data
# - MedicationService: normalize medication and list alternatives
# - AuthorizationService: fetch prior auth rules and limits
# - DecisionLog: persist deterministic decision evidence
#
# Domain terms (lowercase style):
# - invoice, patient, medication, approved_list, normalized_medication
# - quantity_limit, refill_limit, deny_reason, review_reason

# Constants and thresholds
max_mapping_confidence_gap is 0.10
manual_review_required is false
decision_status is "pending"

# -----------------------------------------------------------------------------
# INPUT PHASE
# -----------------------------------------------------------------------------
# The workflow starts when an invoice arrives from a prescription event.
When a medication invoice is submitted:
  # Extract required fields from invoice payload
  Find invoice patient_id and save as patient_id
  Find invoice medication_name and save as medication_name
  Find invoice ndc_code and save as ndc_code
  Find invoice quantity and save as invoice_quantity
  Find invoice refill_count and save as invoice_refill_count
  Find invoice service_date and save as invoice_service_date
  Find invoice prescriber_id and save as prescriber_id

  # ----------------------------------------------------------------------------
  # DATA RETRIEVAL PHASE
  # ----------------------------------------------------------------------------
  # Query patient and authorization context using services.
  Search the PatientService for patient_id
  Search the AuthorizationService for patient_id at invoice_service_date
  Search the MedicationService for medication_name and ndc_code

  # Load approved medications list for this patient + date context.
  Find approved medications for patient_id at invoice_service_date and save as approved_list

  # Normalize medication identity to canonical form.
  Find normalized medication for medication_name and ndc_code and save as normalized_medication

  # ----------------------------------------------------------------------------
  # VALIDATION PHASE: REQUIRED DATA
  # ----------------------------------------------------------------------------
  # Guard clauses prevent undefined decisions.
  If patient_id is missing
    decision_status equals "needs_review"
    review_reason equals "missing_patient_id"
  Otherwise
    decision_status equals decision_status

  If normalized_medication is missing
    decision_status equals "needs_review"
    review_reason equals "unable_to_normalize_medication"
    manual_review_required equals true

  If approved_list is missing
    decision_status equals "needs_review"
    review_reason equals "missing_approved_list"
    manual_review_required equals true

  # ----------------------------------------------------------------------------
  # VALIDATION PHASE: MEDICATION ELIGIBILITY
  # ----------------------------------------------------------------------------
  # Compare canonical medication against patient-approved list.
  If normalized_medication is in approved_list
    decision_status equals "approved"
  Otherwise
    decision_status equals "denied"
    deny_reason equals "medication_not_approved_for_patient"

  # ----------------------------------------------------------------------------
  # LOOP STRUCTURE: CHECK ALTERNATIVE MATCHES
  # ----------------------------------------------------------------------------
  # Some denials can become reviews when there is a near-equivalent mapping
  # (brand/generic ambiguity, coding drift, partial NDC mismatch).
  If decision_status is "denied"
    For each approved_medication in approved_list:
      # Compare similarity score between denied medication and approved item.
      Find similarity score between normalized_medication and approved_medication and save as similarity_score

      # Track best candidate for review escalation.
      If similarity_score is at least 0.90
        manual_review_required equals true
        review_reason equals "possible_equivalent_medication"
      Otherwise
        manual_review_required equals manual_review_required

  # ----------------------------------------------------------------------------
  # LIMIT VALIDATION: QUANTITY + REFILLS
  # ----------------------------------------------------------------------------
  # If currently approved, enforce quantity/refill constraints.
  If decision_status is "approved"
    Find quantity limit for normalized_medication and patient_id and save as quantity_limit
    Find refill limit for normalized_medication and patient_id and save as refill_limit

    If invoice_quantity is greater than quantity_limit
      decision_status equals "denied"
      deny_reason equals "quantity_limit_exceeded"

    If invoice_refill_count is greater than refill_limit
      decision_status equals "denied"
      deny_reason equals "refill_limit_exceeded"

  # ----------------------------------------------------------------------------
  # BOOLEAN COMPOSITION: FINAL ROUTING
  # ----------------------------------------------------------------------------
  # Route ambiguous states to manual review queue.
  If manual_review_required is true and decision_status is not "denied"
    decision_status equals "needs_review"

  # Final safety check: avoid unresolved pending status.
  If decision_status is "pending"
    decision_status equals "needs_review"
    review_reason equals "no_terminal_decision_reached"

  # ----------------------------------------------------------------------------
  # OUTPUT + AUDIT PHASE
  # ----------------------------------------------------------------------------
  # Persist deterministic result with reason and source values.
  Create DecisionLog entry with:
    patient_id
    medication_name
    ndc_code
    normalized_medication
    invoice_quantity
    invoice_refill_count
    invoice_service_date
    decision_status
    deny_reason
    review_reason

  # Notification or queue routing based on terminal status.
  If decision_status is "approved"
    Send Email via NotificationService to billing_team with template "invoice_approved"
  Otherwise
    Route invoice to payment_integrity_review_queue
```

## Notes for Iteration
- This PRD intentionally includes rich inline comments to aid LLM translation quality.
- The Devlish seed demonstrates common language structures:
  - declaration and assignment
  - conditionals and otherwise branches
  - for-each looping
  - boolean composition
  - guard clauses
  - data retrieval actions
  - deterministic terminal routing and audit logging
- During implementation, term syntax should follow the evolving naming thesis:
  - Proper-noun service/type terms for capabilities
  - lowercase domain terms for business concepts
  - dot notation where hierarchical disambiguation is needed
