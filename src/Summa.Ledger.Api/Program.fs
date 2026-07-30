module Summa.Ledger.Api.Program

open System
open System.Text.Json
open Falco
open Falco.Routing
open Microsoft.AspNetCore.Builder
open Microsoft.Extensions.Configuration
open Npgsql
open Summa.Ledger.Domain
open Summa.Ledger.Store

let private toErrorMessage : LedgerError -> string =
    function
    | TooFewEntries -> "a transaction needs at least two entries"
    | NonPositiveAmount -> "every entry amount must be positive"
    | Unbalanced (debits, credits) -> $"debits ({debits}) must equal credits ({credits})"

let private errorResponse statusCode message =
    Response.withStatusCode statusCode
    >> Response.ofJsonOptions Serialization.options { Error = message }

let private postTransaction (store: EventStore) : HttpHandler =
    fun ctx ->
        task {
            try
                let! request = Request.getJsonOptions<PostTransactionRequest> Serialization.options ctx
                let command : PostTransaction =
                    { Id = Guid.NewGuid()
                      OccurredAt = request.OccurredAt
                      Description = request.Description
                      CorrelationId = request.CorrelationId
                      CausationId = request.CausationId
                      IdempotencyKey = request.IdempotencyKey
                      Entries = request.Entries }
                let! result = Handler.handle store command
                return!
                    match result with
                    | Ok id -> Response.ofJsonOptions Serialization.options { Id = id } ctx
                    | Error error -> errorResponse 400 (toErrorMessage error) ctx
            with :? JsonException ->
                return! errorResponse 400 "malformed JSON body" ctx
        }

type Program() = class end

[<EntryPoint>]
let main args =
    let builder = WebApplication.CreateBuilder(args)
    let connectionString = builder.Configuration.GetConnectionString("Summa")
    let dataSource = NpgsqlDataSource.Create(connectionString)
    let store = PostgresEventStore.create dataSource

    let endpoints =
        [ get "/health" (Response.ofPlainText "OK")
          post "/transactions" (postTransaction store) ]

    let wapp = builder.Build()
    wapp.UseRouting()
        .UseFalco(endpoints)
        .Run(Response.withStatusCode 404 >> Response.ofPlainText "not found")
    0
