namespace Summa.Ledger.Store

open System.Text.Json
open System.Text.Json.Serialization
open Summa.Ledger.Domain

module Serialization =

    let private options =
        JsonFSharpOptions.Default()
            .WithUnionUnwrapFieldlessTags()
            .ToJsonSerializerOptions()

    let serialize (transaction: Transaction) : string =
        JsonSerializer.Serialize(transaction, options)

    let deserialize (json: string) : Transaction =
        JsonSerializer.Deserialize<Transaction>(json, options)
