module PolicyStoreTests

open System
open System.Threading.Tasks
open Xunit
open Npgsql
open Npgsql.FSharp
open Summa.Policy.Domain
open Summa.Policy.Store

let private dataSource =
    NpgsqlDataSource.Create "Host=localhost;Port=5432;Username=summa;Password=summa;Database=summa"

let private store = PostgresPolicyStore.create dataSource

let private composedTree =
    Sum [ Flat 5000L
          Sum [ PerUnit(UnitOfMeasure "seat", 200L); Flat 0L ] ]

let private newPolicy idempotencyKey =
    match Policy.create (Guid.NewGuid()) idempotencyKey composedTree with
    | Ok policy -> policy
    | Error e -> failwith $"expected Ok, got Error {e}"

[<Fact>]
let ``a composed pricing tree persists and re-reads identically, including nested Sum`` () : Task =
    task {
        let policy = newPolicy (Guid.NewGuid().ToString())

        let! publishResult = store.Publish policy
        match publishResult with
        | Published -> ()
        | Duplicate _ -> failwith "expected Published, got Duplicate"

        let! readBack = store.Read policy.Id
        match readBack with
        | Some roundTripped ->
            Assert.Equal(policy.Id, roundTripped.Id)
            Assert.Equal(policy.IdempotencyKey, roundTripped.IdempotencyKey)
            Assert.Equal(policy.Pricing, roundTripped.Pricing)
        | None -> failwith "expected the published policy to be readable"
    }

[<Fact>]
let ``publishing the same idempotency key twice returns Duplicate and leaves exactly one row`` () : Task =
    task {
        // A retried authoring reuses the idempotency key but mints a fresh policy id,
        // so the unique idempotency_key constraint — not the policy_id primary key —
        // is what catches the retry.
        let idempotencyKey = Guid.NewGuid().ToString()
        let firstPolicy = newPolicy idempotencyKey
        let secondPolicy = newPolicy idempotencyKey

        let! first = store.Publish firstPolicy
        let! second = store.Publish secondPolicy

        match first, second with
        | Published, Duplicate existingId -> Assert.Equal(firstPolicy.Id, existingId)
        | _ -> failwith $"expected Published then Duplicate, got {first}, {second}"

        let! count =
            dataSource
            |> Sql.fromDataSource
            |> Sql.query "SELECT count(*) AS count FROM policy.policies WHERE idempotency_key = @key;"
            |> Sql.parameters [ "key", Sql.string idempotencyKey ]
            |> Sql.executeRowAsync (fun read -> read.int64 "count")
        Assert.Equal(1L, count)
    }
