namespace Summa.Policy.Domain

open System

/// Companion module to the Policy type (same pattern as Transaction in
/// Summa.Ledger.Domain): the smart constructor that validates a pricing tree, and
/// the pure evaluator that prices supplied quantities into an itemized breakdown.
/// This is domain core — no I/O, no clock, no Guid.NewGuid(); the id is passed in.
///
/// The ModuleSuffix representation lets a module share the Policy type's name in
/// the same namespace (the same pairing FSharp.Core uses for type List / module
/// List); without it the type and module collide across the two files.
[<CompilationRepresentation(CompilationRepresentationFlags.ModuleSuffix)>]
module Policy =

    let private amountOf lineItem =
        match lineItem with
        | FlatCharge amount -> amount
        | PerUnitCharge (_, _, _, amount) -> amount

    /// Number of PerUnit leaves — i.e. how many quantities evaluation requires.
    let rec private perUnitLeafCount comp =
        match comp with
        | Flat _ -> 0
        | PerUnit _ -> 1
        | Sum children -> children |> List.sumBy perUnitLeafCount

    /// Recursively validate the tree, first error wins. A malformed policy is
    /// unrepresentable: only a tree that passes this yields a Policy.
    let rec private validate comp =
        match comp with
        | Flat amount -> if amount < 0L then Error NegativeAmount else Ok()
        | PerUnit (UnitOfMeasure unit, unitPrice) ->
            if String.IsNullOrWhiteSpace unit then Error EmptyUnitOfMeasure
            elif unitPrice < 0L then Error NegativeAmount
            else Ok()
        | Sum [] -> Error EmptySum
        | Sum children ->
            children
            |> List.fold (fun acc child -> acc |> Result.bind (fun () -> validate child)) (Ok())

    /// Smart constructor. The id and idempotency key are supplied by the caller
    /// (the domain core mints neither). Returns the published, immutable Policy on
    /// success, or the first validation error.
    let create id idempotencyKey pricing : Result<Policy, PricingError> =
        validate pricing
        |> Result.map (fun () ->
            { Id = id
              IdempotencyKey = idempotencyKey
              Pricing = pricing })

    /// Price a validated policy against supplied quantities, producing one line
    /// item per leaf (in tree order) and their total. Quantities are consumed
    /// left-to-right, one per PerUnit leaf; a quantity whose unit doesn't match the
    /// leaf it lands on is rejected, as is a quantity count that doesn't match the
    /// number of PerUnit leaves.
    let evaluate (policy: Policy) (quantities: Quantity list) : Result<Breakdown, PricingError> =
        let expected = perUnitLeafCount policy.Pricing
        let actual = List.length quantities
        let countError = Error(QuantityCountMismatch(expected, actual))

        // Returns the leaf's line items plus the quantities still unconsumed, so a
        // Sum can thread the remaining list through its children in order.
        let rec eval comp qs =
            match comp with
            | Flat amount -> Ok([ FlatCharge amount ], qs)
            | PerUnit (unit, unitPrice) ->
                match qs with
                | q :: rest when q.Unit = unit ->
                    Ok([ PerUnitCharge(unit, q.Amount, unitPrice, unitPrice * q.Amount) ], rest)
                | q :: _ -> Error(UnitMismatch(unit, q.Unit))
                | [] -> countError // unreachable once the count guard below passes
            | Sum children ->
                children
                |> List.fold
                    (fun acc child ->
                        acc
                        |> Result.bind (fun (items, remaining) ->
                            eval child remaining
                            |> Result.map (fun (childItems, remaining') -> items @ childItems, remaining')))
                    (Ok([], qs))

        if expected <> actual then
            countError
        else
            eval policy.Pricing quantities
            |> Result.map (fun (items, _) ->
                { LineItems = items
                  Total = items |> List.sumBy amountOf })
