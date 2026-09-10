namespace Summa.Policy.Store

open System
open System.Threading.Tasks
open Summa.Policy.Domain

type PublishResult =
    | Published
    | Duplicate of existingId: Guid

type PolicyStore =
    { Publish : Policy -> Task<PublishResult>
      Read    : Guid -> Task<Policy option> }
