pub mod error;
pub mod trader;
pub mod commander;

use crate::engine::error::EngineError;
use crate::engine::trader::Trader;
use crate::engine::commander::Commander;
use crate::data::handler::{Continuer, MarketGenerator};
use crate::execution::FillGenerator;
use crate::portfolio::repository::PositionHandler;
use crate::portfolio::{FillUpdater, MarketUpdater, OrderGenerator};
use crate::statistic::summary::{PositionSummariser, TablePrinter};
use crate::strategy::SignalGenerator;
use crate::event::{Event, MessageTransmitter};
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{info, warn};
use uuid::Uuid;
use crate::portfolio::position::PositionId;

// Todo:
//  - Impl consistent structured logging in Engine & Trader
//  - Do I need TraderId? Market should probably be enough! Maybe it can have engineId & market
//  - Ensure i'm happy with where event Event & Command live (eg/ Balance is in event.rs)
//  - Add Deserialize to Event.
//  - Search for wrong indented Wheres
//  - Search for todo!() since I found one in /statistic/summary/pnl.rs
//  - Ensure I havn't lost any improvements I had on the other branches!
//  - Add unit test cases for update_from_fill tests (4 of them) which use get & set stats
//  - Make as much stuff Copy as can be - start in Statistics!

//  - Add comments where we see '/// Todo:' or similar



/// Communicates a String is a message associated with a [`Command`].
pub type Message = String;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum Command {
    // Engine Only Commands
    // SendOpenPositions(oneshot::Sender<Result<Vec<Position>, EngineError>>),
    // SendSummary(oneshot::Sender<Result<TradingSummary, EngineError>>),
    // All Traders Command
    Terminate(Message),
    ExitAllPositions,
    // Trader specific
    ExitPosition(PositionId),
}

/// Lego components for constructing an [`Engine`] via the new() constructor method.
#[derive(Debug)]
pub struct EngineLego<EventTx, Statistic, Portfolio, Data, Strategy, Execution>
where
    EventTx: MessageTransmitter<Event> + Debug  + Send,
    Statistic: PositionSummariser + TablePrinter,
    Portfolio: MarketUpdater + OrderGenerator + FillUpdater + Send,
    Data: Continuer + MarketGenerator + Send,
    Strategy: SignalGenerator + Send,
    Execution: FillGenerator + Send,
{
    /// Unique identifier for an [`Engine`] in Uuid v4 format. Used as a unique identifier seed for
    /// the Portfolio, Trader & Positions associated with this [`Engine`].
    pub engine_id: Uuid,
    /// mpsc::Receiver for receiving [`Command`]s from a remote source.
    pub command_rx: mpsc::Receiver<Command>,
    /// Todo:
    pub trader_commander: Commander,
    /// Statistics component that can generate a trading summary based on closed positions.
    pub statistics: Statistic,
    /// Shared-access to a global Portfolio instance.
    pub portfolio: Arc<Mutex<Portfolio>>,
    /// Collection of [`Trader`] instances that can concurrently trade a market pair on it's own thread.
    pub traders: Vec<Trader<EventTx, Portfolio, Data, Strategy, Execution>>,
}

/// Multi-threaded Trading Engine capable of trading with an arbitrary number of [`Trader`] market
/// pairs. Each [`Trader`] operates on it's own thread and has it's own Data Handler, Strategy &
/// Execution Handler, as well as shared access to a global Portfolio instance. A graceful remote
/// shutdown is made possible by sending a [`Message`] to the Engine's broadcast::Receiver
/// termination_rx.
#[derive(Debug)]
pub struct Engine<EventTx, Statistic, Portfolio, Data, Strategy, Execution>
where
    EventTx: MessageTransmitter<Event> + Debug,
    Statistic: PositionSummariser + TablePrinter,
    Portfolio: MarketUpdater + OrderGenerator + FillUpdater + Debug + Send,
    Data: Continuer + MarketGenerator + Debug + Send,
    Strategy: SignalGenerator + Debug + Send,
    Execution: FillGenerator + Debug + Send,
{
    /// Unique identifier for an [`Engine`] in Uuid v4 format. Used as a unique identifier seed for
    /// the Portfolio, Trader & Positions associated with this [`Engine`].
    engine_id: Uuid,
    /// mpsc::Receiver for receiving [`Command`]s from a remote source.
    command_rx: mpsc::Receiver<Command>,
    /// Todo:
    trader_commander: Commander,
    /// Statistics component that can generate a trading summary based on closed positions.
    statistics: Statistic,
    /// Shared-access to a global Portfolio instance that implements [`MarketUpdater`],
    /// [`OrderGenerator`] & [`FillUpdater`].
    portfolio: Arc<Mutex<Portfolio>>,
    /// Collection of [`Trader`] instances that can concurrently trade a market pair on it's own thread.
    traders: Vec<Trader<EventTx, Portfolio, Data, Strategy, Execution>>,
}

impl<EventTx, Statistic, Portfolio, Data, Strategy, Execution>
Engine<EventTx, Statistic, Portfolio, Data, Strategy, Execution>
where
    EventTx: MessageTransmitter<Event> + Debug  + Send + 'static,
    Statistic: PositionSummariser + TablePrinter,
    Portfolio: PositionHandler + MarketUpdater + OrderGenerator + FillUpdater + Debug + Send + 'static,
    Data: Continuer + MarketGenerator + Debug + Send + 'static,
    Strategy: SignalGenerator + Debug + Send + 'static,
    Execution: FillGenerator + Debug + Send + 'static,
{
    /// Constructs a new trading [Engine] instance using the provided [EngineLego].
    pub fn new(lego: EngineLego<EventTx, Statistic, Portfolio, Data, Strategy, Execution>) -> Self {
        Self {
            engine_id: lego.engine_id,
            command_rx: lego.command_rx,
            trader_commander: lego.trader_commander,
            statistics: lego.statistics,
            portfolio: lego.portfolio,
            traders: lego.traders,
        }
    }

    /// Builder to construct [Engine] instances.
    pub fn builder() -> EngineBuilder<EventTx, Statistic, Portfolio, Data, Strategy, Execution> {
        EngineBuilder::new()
    }

    /// Run the trading [Engine]. Spawns a thread for each [Trader] instance in the [Engine] and run
    /// the [Trader] event-loop. Asynchronously awaits a remote shutdown [Message]
    /// via the [Engine]'s termination_rx. After remote shutdown has been initiated, the trading
    /// period's statistics are generated & printed with the provided Statistic component.
    pub async fn run(mut self) {
        // Run each Trader instance on it's own Tokio task
        let traders_stopped_organically = futures::future::join_all(
            self
                .traders
                .into_iter()
                .map(|trader| tokio::spawn(async { trader.run() }))
        );

        loop {
            // Action received commands from remote, or wait for all Traders to stop organically
            tokio::select! {
                _ = traders_stopped_organically => {
                    break;
                },

                command = self.command_rx.recv() => {

                    if let Some(command) = command {
                        match command {
                            Command::Terminate(message) => {
                                // Distribute termination message
                                break;
                            },
                            _ => {
                                todo!()
                            }

                        }

                    } else {
                        // Terminate traders due to dropped receiver
                        break;
                    }
                }

            }
        };

        // // Await remote termination command, or for all Traders to stop organically
        // tokio::select! {
        //     // Traders finish organically
        //     _ = traders_finished => {},
        //
        //     // Engine TerminationMessage received, propagate command to every Trader instance
        //     termination_rx_result = self.termination_rx => {
        //         let termination_message = match termination_rx_result {
        //             Ok(message) => message,
        //             Err(_) => {
        //                 let message = "Remote termination sender dropped - terminating Engine";
        //                 warn!("{}", message);
        //                 message.to_owned()
        //             }
        //         };
        //
        //         if let Err(err) = self.traders_termination_tx.send(termination_message) {
        //             warn!(
        //                 "Error occurred while propagating TerminationMessage to Trader instances: {}",
        //                 err
        //             );
        //         }
        //     }
        // };

        // Unlock Portfolio Mutex to access backtest information
        let mut portfolio = match self.portfolio.lock() {
            Ok(portfolio) => portfolio,
            Err(err) => {
                warn!("Mutex poisoned with error: {}", err);
                err.into_inner()
            }
        };

        // Generate TradingSummary
        match portfolio.get_exited_positions(&Uuid::new_v4()).unwrap() {
            None => info!("Backtest yielded no closed Positions - no TradingSummary available"),
            Some(closed_positions) => {
                self.statistics.generate_summary(&closed_positions);
                self.statistics.print();
            }
        }
    }
}

/// Builder to construct [Engine] instances.
#[derive(Debug)]
pub struct EngineBuilder<EventTx, Statistic, Portfolio, Data, Strategy, Execution>
where
    EventTx: MessageTransmitter<Event> + Debug,
    Statistic: PositionSummariser + TablePrinter,
    Portfolio: MarketUpdater + OrderGenerator + FillUpdater + Debug + Send,
    Data: Continuer + MarketGenerator + Debug + Send,
    Strategy: SignalGenerator + Debug + Send,
    Execution: FillGenerator + Debug + Send,
{
    engine_id: Option<Uuid>,
    command_rx: Option<mpsc::Receiver<Command>>,
    trader_commander: Option<Commander>,
    statistics: Option<Statistic>,
    portfolio: Option<Arc<Mutex<Portfolio>>>,
    traders: Option<Vec<Trader<EventTx, Portfolio, Data, Strategy, Execution>>>,
}

impl<EventTx, Statistic, Portfolio, Data, Strategy, Execution>
EngineBuilder<EventTx, Statistic, Portfolio, Data, Strategy, Execution>
where
    EventTx: MessageTransmitter<Event> + Debug,
    Statistic: PositionSummariser + TablePrinter,
    Portfolio: MarketUpdater + OrderGenerator + FillUpdater + Debug + Send,
    Data: Continuer + MarketGenerator + Debug + Send,
    Strategy: SignalGenerator + Debug + Send,
    Execution: FillGenerator + Debug + Send,
{
    fn new() -> Self {
        Self {
            engine_id: None,
            command_rx: None,
            trader_commander: None,
            statistics: None,
            portfolio: None,
            traders: None,
        }
    }

    pub fn engine_id(self, value: Uuid) -> Self {
        Self {
            engine_id: Some(value),
            ..self
        }
    }

    pub fn command_rx(self, value: mpsc::Receiver<Command>) -> Self {
        Self {
            command_rx: Some(value),
            ..self
        }
    }

    pub fn trader_commander(self, value: Commander) -> Self {
        Self {
            trader_commander: Some(value),
            ..self
        }
    }

    pub fn statistics(self, value: Statistic) -> Self {
        Self {
            statistics: Some(value),
            ..self
        }
    }

    pub fn portfolio(self, value: Arc<Mutex<Portfolio>>) -> Self {
        Self {
            portfolio: Some(value),
            ..self
        }
    }

    pub fn traders(self, value: Vec<Trader<EventTx, Portfolio, Data, Strategy, Execution>>) -> Self {
        Self {
            traders: Some(value),
            ..self
        }
    }

    pub fn build(self) -> Result<Engine<EventTx, Statistic, Portfolio, Data, Strategy, Execution>, EngineError> {
        let engine_id = self.engine_id.ok_or(EngineError::BuilderIncomplete)?;
        let command_rx = self.command_rx.ok_or(EngineError::BuilderIncomplete)?;
        let trader_commander = self.trader_commander.ok_or(EngineError::BuilderIncomplete)?;
        let statistics = self.statistics.ok_or(EngineError::BuilderIncomplete)?;
        let portfolio = self.portfolio.ok_or(EngineError::BuilderIncomplete)?;
        let traders = self.traders.ok_or(EngineError::BuilderIncomplete)?;

        Ok(Engine {
            engine_id,
            command_rx,
            trader_commander,
            statistics,
            portfolio,
            traders,
        })
    }
}