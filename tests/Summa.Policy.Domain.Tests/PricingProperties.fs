module PricingProperties

open System
open FsCheck
open FsCheck.FSharp
open FsCheck.Xunit
open Summa.Policy.Domain

let rec private leafCount comp =
    match comp with
    | Flat _ -> 1
    | PerUnit _ -> 1
    | Sum children -> children |> List.sumBy leafCount

let rec private expectedTotal comp (qs: Quantity list) =
    match comp with
    | Flat amount -> amount, qs
    | PerUnit (_, unitPrice) ->
        match qs with
        | q :: rest -> unitPrice * q.Amount, rest
        | [] -> 0L, []
    | Sum children ->
        children
        |> List.fold
            (fun (running, remaining) child ->
                let sub, remaining' = expectedTotal child remaining
                running + sub, remaining')
            (0L, qs)

type PricingScenario =
    { Policy: Policy
      Quantities: Quantity list }

let private unitGen =
    [ "api_call"; "gb_storage"; "seat"; "message" ]
    |> Gen.elements
    |> Gen.map UnitOfMeasure

let private amountGen = Gen.choose (0, 1_000_000) |> Gen.map int64

let private flatGen = amountGen |> Gen.map (fun a -> Flat a, [])

let private perUnitGen =
    gen {
        let! unit = unitGen
        let! price = amountGen
        let! qty = amountGen
        return PerUnit(unit, price), [ { Unit = unit; Amount = qty } ]
    }

let rec private componentGen size =
    if size <= 0 then
        Gen.oneof [ flatGen; perUnitGen ]
    else
        Gen.oneof [ flatGen; perUnitGen; sumGen size ]

and private sumGen size =
    gen {
        let! n = Gen.choose (1, 3)
        let! children = Gen.listOfLength n (componentGen (size / (n + 1)))
        return Sum(children |> List.map fst), (children |> List.collect snd)
    }

let private scenarioGen =
    Gen.sized (fun size ->
        componentGen size
        |> Gen.map (fun (pricing, quantities) ->
            match Policy.create (Guid.NewGuid()) "idempotency" pricing with
            | Ok policy -> { Policy = policy; Quantities = quantities }
            | Error e -> failwith $"generator produced an invalid policy: {e}"))

type Generators =
    static member PricingScenario() = Arb.fromGen scenarioGen

[<Property(Arbitrary = [| typeof<Generators> |])>]
let ``total equals an independent reference computation of the tree`` (scenario: PricingScenario) =
    match Policy.evaluate scenario.Policy scenario.Quantities with
    | Ok breakdown ->
        let reference, _ = expectedTotal scenario.Policy.Pricing scenario.Quantities
        breakdown.Total = reference
    | Error e -> failwith $"expected Ok, got {e}"

[<Property(Arbitrary = [| typeof<Generators> |])>]
let ``breakdown has exactly one line item per leaf`` (scenario: PricingScenario) =
    match Policy.evaluate scenario.Policy scenario.Quantities with
    | Ok breakdown -> List.length breakdown.LineItems = leafCount scenario.Policy.Pricing
    | Error e -> failwith $"expected Ok, got {e}"

[<Property(Arbitrary = [| typeof<Generators> |])>]
let ``Sum preserves each leaf's identity, never collapsing them`` (left: PricingScenario) (right: PricingScenario) =
    let combined =
        match Policy.create (Guid.NewGuid()) "idempotency" (Sum [ left.Policy.Pricing; right.Policy.Pricing ]) with
        | Ok policy -> policy
        | Error e -> failwith $"invalid combined policy: {e}"

    match Policy.evaluate left.Policy left.Quantities,
          Policy.evaluate right.Policy right.Quantities,
          Policy.evaluate combined (left.Quantities @ right.Quantities)
        with
    | Ok l, Ok r, Ok c -> c.LineItems = l.LineItems @ r.LineItems
    | results -> failwith $"expected all Ok, got {results}"
