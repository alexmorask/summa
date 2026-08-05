module PerformanceObligationTests

open System
open Xunit
open Summa.Recognition.Domain

let private startDate = DateTimeOffset(2026, 1, 1, 0, 0, 0, TimeSpan.Zero)

let private newId () = PerformanceObligationId (Guid.NewGuid())

[<Fact>]
let ``zero period count returns InvalidPeriodCount`` () =
    Assert.Equal(Error InvalidPeriodCount, PerformanceObligation.create (newId ()) 12000L 0 startDate "correlation-1" "causation-1" "idem-1")

[<Fact>]
let ``negative period count returns InvalidPeriodCount`` () =
    Assert.Equal(Error InvalidPeriodCount, PerformanceObligation.create (newId ()) 12000L -1 startDate "correlation-1" "causation-1" "idem-1")

[<Fact>]
let ``non-positive total amount returns NonPositiveTotal`` () =
    Assert.Equal(Error NonPositiveTotal, PerformanceObligation.create (newId ()) 0L 12 startDate "correlation-1" "causation-1" "idem-1")

[<Fact>]
let ``valid inputs succeed`` () =
    let id = newId ()
    match PerformanceObligation.create id 12000L 12 startDate "correlation-1" "causation-1" "idem-1" with
    | Ok obligation ->
        Assert.Equal(id, obligation.Id)
        Assert.Equal(12000L, obligation.TotalAmount)
        Assert.Equal(12, obligation.PeriodCount)
        Assert.Equal(startDate, obligation.StartDate)
        Assert.Equal("correlation-1", obligation.CorrelationId)
        Assert.Equal("causation-1", obligation.CausationId)
        Assert.Equal("idem-1", obligation.IdempotencyKey)
    | Error e -> failwith $"expected Ok, got Error {e}"

let private create total periods idem =
    PerformanceObligation.create (newId ()) total periods startDate "correlation-1" "causation-1" idem
    |> Result.defaultWith (fun _ -> failwith "expected Ok")

[<Fact>]
let ``even division splits the total equally across every period`` () =
    let obligation = create 12000L 12 "idem-1"
    let entries = PerformanceObligation.schedule obligation
    Assert.Equal(12, List.length entries)
    Assert.All(entries, fun entry -> Assert.Equal(1000L, entry.Amount))
    Assert.Equal(12000L, entries |> List.sumBy (fun entry -> entry.Amount))

[<Fact>]
let ``uneven division puts the remainder on the final period`` () =
    let obligation = create 10000L 3 "idem-1"
    let entries = PerformanceObligation.schedule obligation
    Assert.Equal<int64 list>([ 3333L; 3333L; 3334L ], entries |> List.map (fun entry -> entry.Amount))
    Assert.Equal(10000L, entries |> List.sumBy (fun entry -> entry.Amount))

[<Fact>]
let ``schedule is deterministic across repeated calls`` () =
    let obligation = create 10000L 3 "idem-1"
    let first = PerformanceObligation.schedule obligation |> List.map (fun entry -> entry.IdempotencyKey)
    let second = PerformanceObligation.schedule obligation |> List.map (fun entry -> entry.IdempotencyKey)
    Assert.Equal<string list>(first, second)

[<Fact>]
let ``each period's occurred date advances one month from the start date`` () =
    let obligation = create 12000L 12 "idem-1"
    let entries = PerformanceObligation.schedule obligation
    Assert.Equal(startDate, (entries |> List.item 0).OccurredAt)
    Assert.Equal(startDate.AddMonths 2, (entries |> List.item 2).OccurredAt)
