namespace Summa.Policy.Domain

open System

/// Money is stored as integer minor units (cents), never float or decimal.
/// Single currency for now — currency is not yet modeled as a dimension.
type Money = int64

/// An opaque unit-of-measure label for metered pricing (e.g. "api_call",
/// "gb_storage"). Wrapped in a single-case DU so it can't be confused with any
/// other string and so a supplied quantity's unit can be checked against a
/// PerUnit component's expected unit.
type UnitOfMeasure = UnitOfMeasure of string

/// The composable pricing algebra — exactly three operations. Recursive by
/// construction: a bundle is a Sum of sub-components, and a future pricing shape
/// (tiered, volume, a time-boxed discount) becomes a new case here, not new API
/// surface elsewhere.
type PriceComponent =
    | Flat of amount: Money
    | PerUnit of unit: UnitOfMeasure * unitPrice: Money
    | Sum of PriceComponent list

/// A published policy: an immutable envelope carrying identity and the caller's
/// idempotency key (a retried author must not double-create — enforced for real
/// by the database later) around a validated pricing tree. Only produced by
/// Policy.create, so an unvalidated tree can never be evaluated.
type Policy =
    { Id: Guid
      IdempotencyKey: string
      Pricing: PriceComponent }

/// A quantity supplied at evaluation time. Policy never sources usage — quantities
/// are passed in. Tagged with the unit it was metered in so the evaluator can
/// reject a quantity whose unit doesn't match the PerUnit component it lands on.
/// Integer-only for now (fractional usage lands with a future metering context).
type Quantity =
    { Unit: UnitOfMeasure
      Amount: int64 }

/// One line of an itemized breakdown, produced from exactly one leaf of the
/// pricing tree. Leaves are never collapsed into an opaque total, so a bundle can
/// fan out to per-component recognition later.
type LineItem =
    | FlatCharge of amount: Money
    | PerUnitCharge of unit: UnitOfMeasure * quantity: int64 * unitPrice: Money * amount: Money

/// The result of evaluating a policy against supplied quantities: every leaf's
/// line item, in tree order, plus their total.
type Breakdown =
    { LineItems: LineItem list
      Total: Money }

/// Expected, domain-relevant failures the caller must branch on — validation of
/// the tree (create) and of the supplied quantities (evaluate). Modeled as a
/// Result error, never an exception.
type PricingError =
    | EmptySum
    | EmptyUnitOfMeasure
    | NegativeAmount
    | UnitMismatch of expected: UnitOfMeasure * actual: UnitOfMeasure
    | QuantityCountMismatch of expected: int * actual: int
