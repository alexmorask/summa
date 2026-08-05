namespace Summa.Recognition.Domain

open System

type PerformanceObligationId = PerformanceObligationId of Guid

type PerformanceObligation =
    { Id             : PerformanceObligationId
      TotalAmount    : int64
      PeriodCount    : int
      StartDate      : DateTimeOffset
      CorrelationId  : string
      CausationId    : string
      IdempotencyKey : string }

type RecognitionEntry =
    { ObligationId   : PerformanceObligationId
      PeriodIndex    : int
      OccurredAt     : DateTimeOffset
      Amount         : int64
      IdempotencyKey : string }

type ObligationError =
    | InvalidPeriodCount
    | NonPositiveTotal

type ObligationCreated = ObligationCreated of PerformanceObligation

module PerformanceObligation =

    let create id totalAmount periodCount startDate correlationId causationId idempotencyKey =
        if periodCount <= 0 then
            Error InvalidPeriodCount
        elif totalAmount <= 0L then
            Error NonPositiveTotal
        else
            Ok { Id = id
                 TotalAmount = totalAmount
                 PeriodCount = periodCount
                 StartDate = startDate
                 CorrelationId = correlationId
                 CausationId = causationId
                 IdempotencyKey = idempotencyKey }

    let schedule (obligation: PerformanceObligation) : RecognitionEntry list =
        let (PerformanceObligationId rawId) = obligation.Id
        let periods = int64 obligation.PeriodCount
        let baseAmount = obligation.TotalAmount / periods
        let remainder = obligation.TotalAmount - baseAmount * periods
        [ for period in 0 .. obligation.PeriodCount - 1 ->
            let amount =
                if period = obligation.PeriodCount - 1 then baseAmount + remainder
                else baseAmount
            { ObligationId = obligation.Id
              PeriodIndex = period
              OccurredAt = obligation.StartDate.AddMonths period
              Amount = amount
              IdempotencyKey = $"recognition:{rawId}:{period}" } ]
