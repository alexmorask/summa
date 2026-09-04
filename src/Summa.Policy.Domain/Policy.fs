namespace Summa.Policy.Domain

open System

type Money = int64

type UnitOfMeasure = UnitOfMeasure of string

type PricingFormula =
    | Flat of amount: Money
    | PerUnit of unit: UnitOfMeasure * unitPrice: Money
    | Sum of PricingFormula list

type Policy =
    { Id             : Guid
      IdempotencyKey : string
      Pricing        : PricingFormula }

type Usage =
    { Unit   : UnitOfMeasure
      Amount : int64 }

type LineItem =
    | FlatCharge of amount: Money
    | PerUnitCharge of unit: UnitOfMeasure * quantity: int64 * unitPrice: Money

type Breakdown =
    { LineItems : LineItem list
      Total     : Money }

type PricingError =
    | EmptySum
    | EmptyUnitOfMeasure
    | NegativeAmount
    | NegativeQuantity
    | UnitMismatch of expected: UnitOfMeasure * actual: UnitOfMeasure
    | QuantityCountMismatch of required: int * supplied: int

module Policy =

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
        let countError () =
            Error(QuantityCountMismatch(perUnitLeafCount policy.Pricing, List.length usage))

        let rec eval pricing usage =
            match pricing with
            | Flat amount -> Ok([ FlatCharge amount ], amount, usage)
            | PerUnit (unit, unitPrice) ->
                match usage with
                | u :: rest when u.Unit = unit ->
                    if u.Amount < 0L then
                        Error NegativeQuantity
                    else
                        Ok([ PerUnitCharge(unit, u.Amount, unitPrice) ], unitPrice * u.Amount, rest)
                | u :: _ -> Error(UnitMismatch(unit, u.Unit))
                | [] -> countError ()
            | Sum children ->
                (Ok([], 0L, usage), children)
                ||> List.fold (fun acc child ->
                    acc
                    |> Result.bind (fun (itemGroups, total, remaining) ->
                        eval child remaining
                        |> Result.map (fun (childItems, childTotal, remaining') ->
                            childItems :: itemGroups, total + childTotal, remaining')))
                |> Result.map (fun (itemGroups, total, remaining) ->
                    List.concat (List.rev itemGroups), total, remaining)

        eval policy.Pricing usage
        |> Result.bind (fun (items, total, remaining) ->
            if List.isEmpty remaining then
                Ok { LineItems = items; Total = total }
            else
                countError ())
