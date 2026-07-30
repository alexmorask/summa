FROM mcr.microsoft.com/dotnet/sdk:10.0 AS build
WORKDIR /src

COPY src/Summa.Ledger.Domain/Summa.Ledger.Domain.fsproj src/Summa.Ledger.Domain/
COPY src/Summa.Ledger.Store/Summa.Ledger.Store.fsproj src/Summa.Ledger.Store/
COPY src/Summa.Ledger.Projections/Summa.Ledger.Projections.fsproj src/Summa.Ledger.Projections/
RUN dotnet restore src/Summa.Ledger.Projections/Summa.Ledger.Projections.fsproj

COPY src/Summa.Ledger.Domain/ src/Summa.Ledger.Domain/
COPY src/Summa.Ledger.Store/ src/Summa.Ledger.Store/
COPY src/Summa.Ledger.Projections/ src/Summa.Ledger.Projections/
RUN dotnet publish src/Summa.Ledger.Projections/Summa.Ledger.Projections.fsproj -c Release -o /app --no-restore

FROM mcr.microsoft.com/dotnet/runtime:10.0 AS final
WORKDIR /app
COPY --from=build /app .
USER $APP_UID
ENTRYPOINT ["dotnet", "Summa.Ledger.Projections.dll"]
