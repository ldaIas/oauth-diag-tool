-- Copyright 2026 Sam Sovereign
-- SPDX-License-Identifier: Apache-2.0


module ServerForm exposing (Action(..), Model, Msg, init, update, view)

import Html exposing (..)
import Html.Attributes exposing (class, placeholder, type_, value)
import Html.Events exposing (onClick, onInput)


-- MODEL


type alias Model =
    { name : String
    , portInput : String
    , port_ : Int
    , issuerUrl : String
    , authorizationUrl : String
    , tokenEndpoint : String
    }


init : Int -> Model
init port_ =
    { name = ""
    , portInput = String.fromInt port_
    , port_ = port_
    , issuerUrl = ""
    , authorizationUrl = ""
    , tokenEndpoint = ""
    }
        |> recomputeUrls


recomputeUrls : Model -> Model
recomputeUrls model =
    let
        issuer =
            "http://localhost:" ++ String.fromInt model.port_
    in
    { model
        | issuerUrl = issuer
        , authorizationUrl = issuer ++ "/authorize"
        , tokenEndpoint = issuer ++ "/token"
    }


-- UPDATE


type Msg
    = SetName String
    | SetPort String
    | Submit
    | Cancel


type Action
    = SubmitForm
    | CancelForm


update : Msg -> Model -> ( Model, Maybe Action )
update msg model =
    case msg of
        SetName val ->
            ( { model | name = val }, Nothing )

        SetPort val ->
            let
                cleaned =
                    String.filter Char.isDigit val

                port_ =
                    String.toInt cleaned
                        |> Maybe.withDefault model.port_
                        |> clamp 1 65535
            in
            ( { model | portInput = cleaned, port_ = port_ } |> recomputeUrls, Nothing )

        Submit ->
            ( model, Just SubmitForm )

        Cancel ->
            ( model, Just CancelForm )


clamp : Int -> Int -> Int -> Int
clamp lo hi n =
    max lo (min hi n)


-- VIEW


view : Model -> Html Msg
view model =
    div [ class "form-overlay" ]
        [ div [ class "form-panel" ]
            [ div [ class "form-title" ] [ text "New Auth Server" ]
            , viewInput "Server Name" model.name SetName "My Auth Server"
            , viewInput "Port" model.portInput SetPort "9500"
            , viewReadonly "Issuer URL" model.issuerUrl
            , viewReadonly "Authorization URL" model.authorizationUrl
            , viewReadonly "Token Endpoint" model.tokenEndpoint
            , div [ class "form-actions" ]
                [ button [ class "btn-cancel", onClick Cancel ] [ text "Cancel" ]
                , button [ class "btn-submit", onClick Submit ] [ text "Create Server" ]
                ]
            ]
        ]


viewInput : String -> String -> (String -> Msg) -> String -> Html Msg
viewInput label val toMsg hint =
    div [ class "form-field" ]
        [ span [ class "form-label" ] [ text label ]
        , input [ class "form-input", type_ "text", value val, placeholder hint, onInput toMsg ] []
        ]


viewReadonly : String -> String -> Html Msg
viewReadonly label val =
    div [ class "form-field" ]
        [ span [ class "form-label" ] [ text label ]
        , div [ class "form-derived" ] [ text val ]
        ]
