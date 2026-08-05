module Summa.Recognition.Job.Runner

open System
open System.Threading.Tasks
open Summa.Recognition.Domain
open Summa.Recognition.Store
open Summa.Ledger.Domain
open Summa.Ledger.Store

let private toCommand (obligation: PerformanceObligation) (entry: RecognitionEntry) : PostTransaction =
    { Id = Guid.NewGuid()
      OccurredAt = entry.OccurredAt
      Description = "Recognize revenue for period"
      CorrelationId = obligation.CorrelationId
      CausationId = $"RecognitionDue:{entry.IdempotencyKey}"
      IdempotencyKey = entry.IdempotencyKey
      Entries =
        [ { Account = AccountId "deferred_revenue"; Direction = Debit; Amount = entry.Amount }
          { Account = AccountId "revenue"; Direction = Credit; Amount = entry.Amount } ] }

let run (obligationStore: ObligationEventStore) (ledgerStore: EventStore) (now: DateTimeOffset) : Task<unit> =
    task {
        let! obligationEvents = obligationStore.ReadFrom 0L
        for stored in obligationEvents do
            let (ObligationCreated obligation) = stored.Event
            let due = PerformanceObligation.schedule obligation |> List.filter (fun entry -> entry.OccurredAt <= now)
            for entry in due do
                let! _ = Handler.handle ledgerStore (toCommand obligation entry)
                ()
    }
