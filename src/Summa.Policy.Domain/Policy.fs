namespace Summa.Policy.Domain

open System

type Money = int64

type UnitOfMeasure = UnitOfMeasure of string

type PricingFormula =
    | Flat of amount: Money
    | PerUnit of unit: UnitOfMeasure * unitPrice: Money
    | Sum of PricingFormula list

type Policy =
    { Id: Guid
      IdempotencyKey: string
      Pricing: PricingFormula }

type Usage =
    { Unit: UnitOfMeasure
      Amount: int64 }

type LineItem =
    | FlatCharge of amount: Money
    | PerUnitCharge of unit: UnitOfMeasure * quantity: int64 * unitPrice: Money

type Breakdown =
    { LineItems: LineItem list
      Total: Money }

type PricingError =
    | EmptySum
    | EmptyUnitOfMeasure
    | NegativeAmount
    | NegativeQuantity
    | UnitMismatch of expected: UnitOfMeasure * actual: UnitOfMeasure
    | QuantityCountMismatch of required: int * supplied: int

module Policy =

    let private amountOf lineItem =
        match lineItem with
        | FlatCharge amount -> amount
        | PerUnitCharge (_, quantity, unitPrice) -> unitPrice * quantity

    let rec private perUnitLeafCount pricing =
        match pricing with
        | Flat _ -> 0
        | PerUnit _ -> 1
        | Sum children -> children |> List.sumBy perUnitLeafCount

    let rec private validate pricing =
        match pricing with
        | Flat amount -> if amount < 0L then Error NegativeAmount else Ok()
        | PerUnit (UnitOfMeasure unit, unitPrice) ->
            if String.IsNullOrWhiteSpace unit then Error EmptyUnitOfMeasure
            elif unitPrice < 0L then Error NegativeAmount
            else Ok()
        | Sum [] -> Error EmptySum
        | Sum children ->
            (Ok(), children)
            ||> List.fold (fun acc child -> acc |> Result.bind (fun () -> validate child))

    let create id idempotencyKey pricing : Result<Policy, PricingError> =
        validate pricing
        |> Result.map (fun () ->
            { Id = id
              IdempotencyKey = idempotencyKey
              Pricing = pricing })

    let evaluate (policy: Policy) (usage: Usage list) : Result<Breakdown, PricingError> =
        let requiredUsageCount = perUnitLeafCount policy.Pricing
        let suppliedUsageCount = List.length usage
        let countError = Error(QuantityCountMismatch(requiredUsageCount, suppliedUsageCount))

        let rec eval pricing usage =
            match pricing with
            | Flat amount -> Ok([ FlatCharge amount ], usage)
            | PerUnit (unit, unitPrice) ->
                match usage with
                | u :: rest when u.Unit = unit ->
                    if u.Amount < 0L then
                        Error NegativeQuantity
                    else
                        Ok([ PerUnitCharge(unit, u.Amount, unitPrice) ], rest)
                | u :: _ -> Error(UnitMismatch(unit, u.Unit))
                | [] -> countError
            | Sum children ->
                children
                |> List.fold
                    (fun acc child ->
                        acc
                        |> Result.bind (fun (items, remaining) ->
                            eval child remaining
                            |> Result.map (fun (childItems, remaining') -> items @ childItems, remaining')))
                    (Ok([], usage))

        if requiredUsageCount <> suppliedUsageCount then
            countError
        else
            eval policy.Pricing usage
            |> Result.map (fun (items, _) ->
                { LineItems = items
                  Total = items |> List.sumBy amountOf })
