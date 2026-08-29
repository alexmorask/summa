module PolicyTests

open System
open Xunit
open Summa.Policy.Domain

let private apiCalls = UnitOfMeasure "api_call"

let private create pricing =
    Policy.create (Guid.NewGuid()) "idempotency-1" pricing

let private policyOf pricing =
    match create pricing with
    | Ok policy -> policy
    | Error e -> failwith $"invalid test policy: {e}"

let private evaluate pricing usage =
    Policy.evaluate (policyOf pricing) usage

[<Fact>]
let ``flat and per-unit compose into a valid policy`` () =
    let pricing = Sum [ Flat 1000L; PerUnit(apiCalls, 5L) ]
    match create pricing with
    | Ok policy -> Assert.Equal(pricing, policy.Pricing)
    | Error e -> failwith $"expected Ok, got {e}"

[<Fact>]
let ``empty Sum is rejected`` () =
    Assert.Equal(Error EmptySum, create (Sum []))

[<Fact>]
let ``blank unit of measure is rejected`` () =
    Assert.Equal(Error EmptyUnitOfMeasure, create (PerUnit(UnitOfMeasure "  ", 5L)))

[<Fact>]
let ``negative flat amount is rejected`` () =
    Assert.Equal(Error NegativeAmount, create (Flat -1L))

[<Fact>]
let ``negative unit price is rejected`` () =
    Assert.Equal(Error NegativeAmount, create (PerUnit(apiCalls, -5L)))

[<Fact>]
let ``zero amount is allowed as a free component`` () =
    match create (Flat 0L) with
    | Ok _ -> ()
    | Error e -> failwith $"expected Ok, got {e}"

[<Fact>]
let ``flat evaluates to a single flat charge with no quantities`` () =
    match evaluate (Flat 1000L) [] with
    | Ok breakdown ->
        Assert.Equal<LineItem list>([ FlatCharge 1000L ], breakdown.LineItems)
        Assert.Equal(1000L, breakdown.Total)
    | Error e -> failwith $"expected Ok, got {e}"

[<Fact>]
let ``per-unit multiplies unit price by the supplied quantity`` () =
    let usage = [ { Unit = apiCalls; Amount = 2000L } ]
    match evaluate (PerUnit(apiCalls, 5L)) usage with
    | Ok breakdown ->
        Assert.Equal<LineItem list>([ PerUnitCharge(apiCalls, 2000L, 5L, 10000L) ], breakdown.LineItems)
        Assert.Equal(10000L, breakdown.Total)
    | Error e -> failwith $"expected Ok, got {e}"

[<Fact>]
let ``nested Sum itemizes every leaf in order and totals them`` () =
    let pricing = Sum [ Flat 1000L; PerUnit(apiCalls, 5L) ]
    let usage = [ { Unit = apiCalls; Amount = 100L } ]
    match evaluate pricing usage with
    | Ok breakdown ->
        Assert.Equal<LineItem list>(
            [ FlatCharge 1000L; PerUnitCharge(apiCalls, 100L, 5L, 500L) ],
            breakdown.LineItems
        )
        Assert.Equal(1500L, breakdown.Total)
    | Error e -> failwith $"expected Ok, got {e}"

[<Fact>]
let ``a zero-amount leaf itemizes as a zero charge`` () =
    let pricing = Sum [ Flat 0L; PerUnit(apiCalls, 5L) ]
    let usage = [ { Unit = apiCalls; Amount = 0L } ]
    match evaluate pricing usage with
    | Ok breakdown ->
        Assert.Equal<LineItem list>(
            [ FlatCharge 0L; PerUnitCharge(apiCalls, 0L, 5L, 0L) ],
            breakdown.LineItems
        )
        Assert.Equal(0L, breakdown.Total)
    | Error e -> failwith $"expected Ok, got {e}"

[<Fact>]
let ``a quantity whose unit doesn't match the leaf is rejected`` () =
    let gb = UnitOfMeasure "gb_storage"
    let usage = [ { Unit = gb; Amount = 100L } ]
    Assert.Equal(Error(UnitMismatch(apiCalls, gb)), evaluate (PerUnit(apiCalls, 5L)) usage)

[<Fact>]
let ``a negative quantity is rejected`` () =
    let usage = [ { Unit = apiCalls; Amount = -1L } ]
    Assert.Equal(Error NegativeQuantity, evaluate (PerUnit(apiCalls, 5L)) usage)

[<Fact>]
let ``too few quantities is rejected as a count mismatch`` () =
    Assert.Equal(Error(QuantityCountMismatch(1, 0)), evaluate (PerUnit(apiCalls, 5L)) [])

[<Fact>]
let ``too many quantities is rejected as a count mismatch`` () =
    let usage =
        [ { Unit = apiCalls; Amount = 1L }
          { Unit = apiCalls; Amount = 2L } ]

    Assert.Equal(Error(QuantityCountMismatch(1, 2)), evaluate (PerUnit(apiCalls, 5L)) usage)
