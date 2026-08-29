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

type Quantity =
    { Unit: UnitOfMeasure
      Amount: int64 }

type LineItem =
    | FlatCharge of amount: Money
    | PerUnitCharge of unit: UnitOfMeasure * quantity: int64 * unitPrice: Money * amount: Money

type Breakdown =
    { LineItems: LineItem list
      Total: Money }

type PricingError =
    | EmptySum
    | EmptyUnitOfMeasure
    | NegativeAmount
    | NegativeQuantity
    | UnitMismatch of expected: UnitOfMeasure * actual: UnitOfMeasure
    | QuantityCountMismatch of expected: int * actual: int

module Policy =

    let private amountOf lineItem =
        match lineItem with
        | FlatCharge amount -> amount
        | PerUnitCharge (_, _, _, amount) -> amount

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

    let evaluate (policy: Policy) (quantities: Quantity list) : Result<Breakdown, PricingError> =
        let expected = perUnitLeafCount policy.Pricing
        let actual = List.length quantities
        let countError = Error(QuantityCountMismatch(expected, actual))

        let rec eval pricing quantities =
            match pricing with
            | Flat amount -> Ok([ FlatCharge amount ], quantities)
            | PerUnit (unit, unitPrice) ->
                match quantities with
                | q :: rest when q.Unit = unit ->
                    if q.Amount < 0L then
                        Error NegativeQuantity
                    else
                        Ok([ PerUnitCharge(unit, q.Amount, unitPrice, unitPrice * q.Amount) ], rest)
                | q :: _ -> Error(UnitMismatch(unit, q.Unit))
                | [] -> countError
            | Sum children ->
                children
                |> List.fold
                    (fun acc child ->
                        acc
                        |> Result.bind (fun (items, remaining) ->
                            eval child remaining
                            |> Result.map (fun (childItems, remaining') -> items @ childItems, remaining')))
                    (Ok([], quantities))

        if expected <> actual then
            countError
        else
            eval policy.Pricing quantities
            |> Result.map (fun (items, _) ->
                { LineItems = items
                  Total = items |> List.sumBy amountOf })
