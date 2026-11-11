-- Copyright 2026 Sam Sovereign
-- SPDX-License-Identifier: Apache-2.0


module ClientConfigFormTests exposing (suite)

import ClientConfigForm exposing (init, initFromEdit, initFromImport, removeAt, toConfigFields, updateAt)
import ClientConfigList exposing (ClientConfig)
import Dict
import Expect
import Test exposing (Test, describe, test)


sampleConfig : ClientConfig
sampleConfig =
    { id = 7
    , name = "my client"
    , issuerUrl = "http://localhost:9500"
    , authorizationUrl = "http://localhost:9500/authorize"
    , tokenUrl = "http://localhost:9500/token"
    , clientId = "client-abc"
    , clientSecret = "secret-xyz"
    , scopes = "openid offline_access"
    , grantType = "client_credentials"
    , extraParams = Dict.fromList [ ( "audience", "api" ) ]
    , disabledParams = Dict.fromList [ ( "scope", True ) ]
    , disabledTokenParams = Dict.fromList [ ( "client_secret", True ) ]
    , scopesSupported = "openid profile"
    , refreshToken = "rt-12345"
    , disabledRefreshParams = Dict.fromList [ ( "scope", True ) ]
    }


suite : Test
suite =
    describe "ClientConfigForm"
        [ describe "initFromEdit"
            [ test "round-trips through toConfigFields without losing fields" <|
                \_ ->
                    (initFromEdit sampleConfig |> toConfigFields)
                        |> Expect.equal
                            { name = sampleConfig.name
                            , issuerUrl = sampleConfig.issuerUrl
                            , authorizationUrl = sampleConfig.authorizationUrl
                            , tokenUrl = sampleConfig.tokenUrl
                            , clientId = sampleConfig.clientId
                            , clientSecret = sampleConfig.clientSecret
                            , scopes = sampleConfig.scopes
                            , grantType = sampleConfig.grantType
                            , extraParams = sampleConfig.extraParams
                            , disabledParams = sampleConfig.disabledParams
                            , disabledTokenParams = sampleConfig.disabledTokenParams
                            , scopesSupported = sampleConfig.scopesSupported
                            , disabledRefreshParams = sampleConfig.disabledRefreshParams
                            }
            , test "marks the form as editing the config's id" <|
                \_ ->
                    (initFromEdit sampleConfig).editingId
                        |> Expect.equal (Just 7)
            , test "converts extraParams into key/value rows" <|
                \_ ->
                    (initFromEdit sampleConfig).extraParams
                        |> Expect.equal [ { key = "audience", value = "api" } ]
            ]
        , describe "toConfigFields"
            [ test "drops extra params with empty keys" <|
                \_ ->
                    let
                        model =
                            initFromEdit sampleConfig
                    in
                    { model | extraParams = model.extraParams ++ [ { key = "", value = "orphan" } ] }
                        |> toConfigFields
                        |> .extraParams
                        |> Expect.equal (Dict.fromList [ ( "audience", "api" ) ])
            ]
        , describe "initFromImport"
            [ test "copies the imported fields and keeps init defaults for the rest" <|
                \_ ->
                    let
                        model =
                            initFromImport
                                { name = "imported"
                                , issuerUrl = "http://localhost:9501"
                                , authorizationUrl = "http://localhost:9501/authorize"
                                , tokenUrl = "http://localhost:9501/token"
                                , clientId = "cid"
                                , clientSecret = "csecret"
                                }
                    in
                    model
                        |> Expect.equal
                            { init
                                | name = "imported"
                                , issuerUrl = "http://localhost:9501"
                                , authorizationUrl = "http://localhost:9501/authorize"
                                , tokenUrl = "http://localhost:9501/token"
                                , clientId = "cid"
                                , clientSecret = "csecret"
                            }
            , test "defaults to the authorization_code grant" <|
                \_ ->
                    init.grantType
                        |> Expect.equal "authorization_code"
            ]
        , describe "removeAt"
            [ test "removes the element at the index" <|
                \_ ->
                    removeAt 1 [ "a", "b", "c" ]
                        |> Expect.equal [ "a", "c" ]
            , test "removes the first and last elements" <|
                \_ ->
                    ( removeAt 0 [ "a", "b" ], removeAt 1 [ "a", "b" ] )
                        |> Expect.equal ( [ "b" ], [ "a" ] )
            , test "is a no-op when the index is out of range" <|
                \_ ->
                    ( removeAt 5 [ "a" ], removeAt -1 [ "a" ], removeAt 0 [] )
                        |> Expect.equal ( [ "a" ], [ "a" ], [] )
            ]
        , describe "updateAt"
            [ test "applies the function at the index only" <|
                \_ ->
                    updateAt 1 String.toUpper [ "a", "b", "c" ]
                        |> Expect.equal [ "a", "B", "c" ]
            , test "is a no-op when the index is out of range" <|
                \_ ->
                    updateAt 5 String.toUpper [ "a" ]
                        |> Expect.equal [ "a" ]
            ]
        ]
