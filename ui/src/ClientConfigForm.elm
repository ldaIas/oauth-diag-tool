-- Copyright 2026 Sam Sovereign
-- SPDX-License-Identifier: Apache-2.0


module ClientConfigForm exposing (Action(..), Model, Msg, init, initFromEdit, initFromImport, toConfigFields, update, view)

import ClientConfigList exposing (ClientConfig)
import Dict exposing (Dict)
import Html exposing (..)
import Html.Attributes exposing (checked, class, placeholder, selected, type_, value)
import Html.Events exposing (onClick, onInput)


-- MODEL


type alias ExtraParam =
    { key : String
    , value : String
    }


type alias Model =
    { editingId : Maybe Int
    , name : String
    , issuerUrl : String
    , authorizationUrl : String
    , tokenUrl : String
    , clientId : String
    , clientSecret : String
    , scopes : String
    , grantType : String
    , extraParams : List ExtraParam
    , disabledParams : Dict String Bool
    , disabledTokenParams : Dict String Bool
    , scopesSupported : String
    , disabledRefreshParams : Dict String Bool
    , useServerMetadata : Bool
    }


init : Model
init =
    { editingId = Nothing
    , name = ""
    , issuerUrl = ""
    , authorizationUrl = ""
    , tokenUrl = ""
    , clientId = ""
    , clientSecret = ""
    , scopes = ""
    , grantType = "authorization_code"
    , extraParams = []
    , disabledParams = Dict.empty
    , disabledTokenParams = Dict.empty
    , scopesSupported = ""
    , disabledRefreshParams = Dict.empty
    , useServerMetadata = False
    }


initFromImport : { name : String, issuerUrl : String, authorizationUrl : String, tokenUrl : String, clientId : String, clientSecret : String } -> Model
initFromImport data =
    { init
        | name = data.name
        , issuerUrl = data.issuerUrl
        , authorizationUrl = data.authorizationUrl
        , tokenUrl = data.tokenUrl
        , clientId = data.clientId
        , clientSecret = data.clientSecret
    }


initFromEdit : ClientConfig -> Model
initFromEdit config =
    { editingId = Just config.id
    , name = config.name
    , issuerUrl = config.issuerUrl
    , authorizationUrl = config.authorizationUrl
    , tokenUrl = config.tokenUrl
    , clientId = config.clientId
    , clientSecret = config.clientSecret
    , scopes = config.scopes
    , grantType = config.grantType
    , extraParams = Dict.toList config.extraParams |> List.map (\( k, v ) -> { key = k, value = v })
    , disabledParams = config.disabledParams
    , disabledTokenParams = config.disabledTokenParams
    , scopesSupported = config.scopesSupported
    , disabledRefreshParams = config.disabledRefreshParams
    , useServerMetadata = False
    }


{-| Convert the form state into the canonical field record accepted by
`ClientConfigList.encodeConfigFields`.
-}
toConfigFields :
    Model
    ->
        { name : String
        , issuerUrl : String
        , authorizationUrl : String
        , tokenUrl : String
        , clientId : String
        , clientSecret : String
        , scopes : String
        , grantType : String
        , extraParams : Dict String String
        , disabledParams : Dict String Bool
        , disabledTokenParams : Dict String Bool
        , scopesSupported : String
        , disabledRefreshParams : Dict String Bool
        }
toConfigFields model =
    { name = model.name
    , issuerUrl = model.issuerUrl
    , authorizationUrl = model.authorizationUrl
    , tokenUrl = model.tokenUrl
    , clientId = model.clientId
    , clientSecret = model.clientSecret
    , scopes = model.scopes
    , grantType = model.grantType
    , extraParams =
        model.extraParams
            |> List.filter (\p -> not (String.isEmpty p.key))
            |> List.map (\p -> ( p.key, p.value ))
            |> Dict.fromList
    , disabledParams = model.disabledParams
    , disabledTokenParams = model.disabledTokenParams
    , scopesSupported = model.scopesSupported
    , disabledRefreshParams = model.disabledRefreshParams
    }



-- UPDATE


type Msg
    = SetName String
    | SetIssuerUrl String
    | SetAuthorizationUrl String
    | SetTokenUrl String
    | SetClientId String
    | SetClientSecret String
    | SetScopes String
    | SetGrantType String
    | AddExtraParam
    | RemoveExtraParam Int
    | SetExtraParamKey Int String
    | SetExtraParamValue Int String
    | ToggleUseServerMetadata
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

        SetIssuerUrl val ->
            ( { model | issuerUrl = val }, Nothing )

        SetAuthorizationUrl val ->
            ( { model | authorizationUrl = val }, Nothing )

        SetTokenUrl val ->
            ( { model | tokenUrl = val }, Nothing )

        SetClientId val ->
            ( { model | clientId = val }, Nothing )

        SetClientSecret val ->
            ( { model | clientSecret = val }, Nothing )

        SetScopes val ->
            ( { model | scopes = val }, Nothing )

        SetGrantType val ->
            ( { model | grantType = val }, Nothing )

        ToggleUseServerMetadata ->
            ( { model | useServerMetadata = not model.useServerMetadata }, Nothing )

        AddExtraParam ->
            ( { model | extraParams = model.extraParams ++ [ { key = "", value = "" } ] }, Nothing )

        RemoveExtraParam idx ->
            ( { model | extraParams = removeAt idx model.extraParams }, Nothing )

        SetExtraParamKey idx val ->
            ( { model | extraParams = updateAt idx (\p -> { p | key = val }) model.extraParams }, Nothing )

        SetExtraParamValue idx val ->
            ( { model | extraParams = updateAt idx (\p -> { p | value = val }) model.extraParams }, Nothing )

        Submit ->
            ( model, Just SubmitForm )

        Cancel ->
            ( model, Just CancelForm )


removeAt : Int -> List a -> List a
removeAt idx list =
    List.indexedMap Tuple.pair list
        |> List.filterMap
            (\( i, item ) ->
                if i == idx then
                    Nothing

                else
                    Just item
            )


updateAt : Int -> (a -> a) -> List a -> List a
updateAt idx fn list =
    List.indexedMap
        (\i item ->
            if i == idx then
                fn item

            else
                item
        )
        list



-- VIEW


view : String -> Model -> Html Msg
view callbackUrl model =
    let
        ( title, submitLabel ) =
            case model.editingId of
                Just _ ->
                    ( "Edit Client Configuration", "Save Config" )

                Nothing ->
                    ( "New Client Configuration", "Create Config" )
    in
    div [ class "form-overlay" ]
        [ div [ class "form-panel" ]
            [ div [ class "form-title" ] [ text title ]
            , viewInput "Name" model.name SetName "My Client Config"
            , viewInput "Issuer URL" model.issuerUrl SetIssuerUrl "https://auth.example.com"
            , div [ class "form-field form-field-checkbox" ]
                [ label [ class "param-toggle" ]
                    [ input [ type_ "checkbox", checked model.useServerMetadata, onClick ToggleUseServerMetadata ] []
                    , span [ class "param-toggle-name" ] [ text "Use Authorization Server Metadata" ]
                    ]
                ]
            , if model.useServerMetadata then
                viewDisabledInput "Authorization URL" model.authorizationUrl "Resolved from metadata"

              else
                viewInput "Authorization URL" model.authorizationUrl SetAuthorizationUrl "https://auth.example.com/authorize"
            , if model.useServerMetadata then
                viewDisabledInput "Token URL" model.tokenUrl "Resolved from metadata"

              else
                viewInput "Token URL" model.tokenUrl SetTokenUrl "https://auth.example.com/token"
            , viewInput "Client ID" model.clientId SetClientId "client-id"
            , viewInput "Client Secret" model.clientSecret SetClientSecret "client-secret"
            , viewInput "Scopes" model.scopes SetScopes "openid profile email"
            , viewGrantType model.grantType
            , if model.grantType == "authorization_code" && not (String.isEmpty callbackUrl) then
                viewReadonlyField "Callback URL" callbackUrl

              else
                text ""
            , viewExtraParams model.extraParams
            , div [ class "form-actions" ]
                [ button [ class "btn-cancel", onClick Cancel ] [ text "Cancel" ]
                , button [ class "btn-submit", onClick Submit ] [ text submitLabel ]
                ]
            ]
        ]


viewInput : String -> String -> (String -> Msg) -> String -> Html Msg
viewInput label val toMsg hint =
    div [ class "form-field" ]
        [ span [ class "form-label" ] [ text label ]
        , input [ class "form-input", type_ "text", value val, placeholder hint, onInput toMsg ] []
        ]


viewDisabledInput : String -> String -> String -> Html Msg
viewDisabledInput label val hint =
    div [ class "form-field" ]
        [ span [ class "form-label" ] [ text label ]
        , input [ class "form-input form-input-readonly", type_ "text", value val, placeholder hint, Html.Attributes.disabled True ] []
        ]


viewReadonlyField : String -> String -> Html Msg
viewReadonlyField label val =
    div [ class "form-field" ]
        [ span [ class "form-label" ] [ text label ]
        , input [ class "form-input form-input-readonly", type_ "text", value val, Html.Attributes.readonly True ] []
        ]


viewGrantType : String -> Html Msg
viewGrantType current =
    div [ class "form-field" ]
        [ span [ class "form-label" ] [ text "Grant Type" ]
        , Html.select [ class "form-input form-select", onInput SetGrantType ]
            [ option [ value "authorization_code", selected (current == "authorization_code") ] [ text "Authorization Code" ]
            , option [ value "client_credentials", selected (current == "client_credentials") ] [ text "Client Credentials" ]
            ]
        ]


viewExtraParams : List ExtraParam -> Html Msg
viewExtraParams params =
    div [ class "form-field" ]
        [ div [ class "extra-params-header" ]
            [ span [ class "form-label" ] [ text "Extra Query Parameters" ]
            , button [ class "btn-add-client", onClick AddExtraParam ] [ text "+ Add" ]
            ]
        , div [ class "extra-params-list" ]
            (List.indexedMap viewExtraParam params)
        ]


viewExtraParam : Int -> ExtraParam -> Html Msg
viewExtraParam idx param =
    div [ class "extra-param-row" ]
        [ input [ class "form-input extra-param-key", type_ "text", value param.key, placeholder "key", onInput (SetExtraParamKey idx) ] []
        , input [ class "form-input extra-param-value", type_ "text", value param.value, placeholder "value", onInput (SetExtraParamValue idx) ] []
        , button [ class "btn-delete", onClick (RemoveExtraParam idx) ] [ text "\u{1F5D1}" ]
        ]
